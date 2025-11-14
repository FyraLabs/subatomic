package rpm

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path"
	"path/filepath"

	"github.com/FyraLabs/subatomic/server/logging"
	"github.com/FyraLabs/subatomic/server/tetsudou"
	"github.com/go-kit/log"
	"github.com/go-kit/log/level"

	pgp "github.com/ProtonMail/gopenpgp/v2/crypto"
	"github.com/sassoftware/go-rpmutils"
	"gopkg.in/ini.v1"
)

var logger = log.With(logging.Logger, "module", "rpm")

type MRepoBatchFile struct {
	AppStreamData MRepoCBatchData `ini:"appstream"`
	Icons         MRepoCBatchData `ini:"appstream-icons"`
}

// TOML struct for modifyrepo_c batch scripts
//
// [path/to/file]
// ...
type MRepoCBatchData struct {
	Path              string `ini:"path"`
	Type              string `ini:"type,omitempty"`
	Remove            bool   `ini:"remove,omitempty"`
	Compress          bool   `ini:"compress"`
	CompressType      string `ini:"compress-type,omitempty"`
	Checksum          string `ini:"checksum,omitempty"`
	UniqueMdFileNames bool   `ini:"unique-md-filenames,omitempty"`
	NewName           string `ini:"new-name"`
}

func CreateRepo(repoPath string) error {
	if err := os.MkdirAll(repoPath, os.ModePerm); err != nil {
		return nil
	}

	if _, err := exec.Command("createrepo_c", repoPath).Output(); err != nil {
		return err
	}

	if err := writeTetsudouMetadata(repoPath); err != nil {
		return err
	}

	return nil
}

func UpdateRepo(repoPath string) error {

	flags := []string{"--update", "--zck", "--xz", "--local-sqlite"}

	_, err := os.Stat(path.Join(repoPath, "comps.xml"))
	if err != nil && !os.IsNotExist(err) {
		return err
	}

	if err == nil {
		flags = append(flags, "--groupfile", "comps.xml")
	}

	flags = append(flags, repoPath)

	// This will only remove something if the directory was in a broken state before
	// Callers of UpdateRepo are expected to lock calls, so we make the assumption this is safe
	_ = os.RemoveAll(path.Join(repoPath, ".repodata"))

	level.Info(logger).Log("msg", "running createrepo_c", "flags", flags)
	if _, err := exec.Command("createrepo_c", flags...).Output(); err != nil {
		if err, ok := err.(*exec.ExitError); ok {
			_ = os.RemoveAll(path.Join(repoPath, ".repodata"))
			return fmt.Errorf("createrepo_c returned non-zero exit code with output '%s': %w", string(err.Stderr), err)
		}

		return err
	}
	level.Info(logger).Log("msg", "createrepo_c completed successfully")

	appstreamDirEnv := os.Getenv("SUBATOMIC_APPSTREAM_DIR")
	if appstreamDirEnv != "" {
		level.Info(logger).Log("msg", "modifying repo appstream metadata from directory", "dir", appstreamDirEnv)
		appstreamDir, err := filepath.Abs(appstreamDirEnv)
		if err != nil {
			return err
		}

		if err := ModifyRepoAppStream(repoPath, appstreamDir); err != nil {
			level.Error(logger).Log("msg", "failed to modify repo appstream, failing silently", "error_msg", err.Error())
		}
		
		level.Info(logger).Log("msg", "modified repo appstream metadata successfully")
	}

	if err := writeTetsudouMetadata(repoPath); err != nil {
		return err
	}

	return nil
}

func writeTetsudouMetadata(repoPath string) error {
	// We calculate and write some metadata for Tetsudou, which is our mirroring system
	// This is not strictly necessary for the repo to function, but it's useful for our use case (and possibly others)
	repomd, err := os.Open(path.Join(repoPath, "repodata/repomd.xml"))
	if err != nil {
		return err
	}
	defer repomd.Close()

	repodata, err := tetsudou.RepodataFromFile(repomd)
	if err != nil {
		return err
	}

	tetsudouJson, err := json.Marshal(repodata)
	if err != nil {
		return err
	}

	if err := os.WriteFile(path.Join(repoPath, "repodata/tetsudou.json"), tetsudouJson, 0644); err != nil {
		return err
	}

	return nil
}

// use `modifyrepo_c` to update AppStream metadata in the repo
//
// Expects the base path of the repo to be <repo>/latest, with the tree structure of:
// <repo>/latest/
//
//	   appstream/
//			<repo>.xml.gz
//			<repo>-icons.tar.gz
//	   icons/
//			x64x64/
//				<icon files>
//			x128x128/
//				<icon files>
func MrepoCConfig(repoPath string, appstreamPath string) (*string, error) {
	level.Debug(logger).Log("msg", "Generating mrepo_c config for repo", "repoPath", repoPath, "appstreamPath", appstreamPath)
	repoName := path.Base(repoPath)

	batchTemplate := MRepoCBatchData{
		Compress: true,
	}

	// [appstream]
	appstreamConfig := batchTemplate
	appstreamFile := path.Join(appstreamPath, repoName, "latest/appstream", fmt.Sprintf("%s.xml.gz", repoName))
	appstreamConfig.Path = appstreamFile
	appstreamConfig.NewName = "appstream.xml"

	// [icons]
	iconsConfig := batchTemplate
	iconsFile := path.Join(appstreamPath, repoName, "latest/appstream", fmt.Sprintf("%s-icons-64x64.tar.gz", repoName))
	iconsConfig.Path = iconsFile
	iconsConfig.Type = "appstream-icons"
	iconsConfig.NewName = "appstream-icons-64x64.tar"

	repoBatch := MRepoBatchFile{
		// AppStreamData: appstreamConfig,
		// Icons:         iconsConfig,
	}
	if _, err := os.Stat(iconsFile); err == nil {
		repoBatch.Icons = iconsConfig
	}

	if _, err := os.Stat(appstreamFile); err == nil {
		repoBatch.AppStreamData = appstreamConfig
	}
	ini.PrettyFormat = false

	inifile := ini.Empty()
	inifile.ReflectFrom(&repoBatch)

	configFileName := fmt.Sprintf("%s-mrepoc.ini", repoName)
	configPath := path.Join("/tmp", configFileName)
	if err := inifile.SaveTo(configPath); err != nil {
		return nil, err
	}
	level.Debug(logger).Log("msg", "Generated mrepo_c config", "configPath", configPath)

	return &configPath, nil
}

func ModifyRepoAppStream(repoPath string, appstreamPath string) error {
	level.Debug(logger).Log("msg", "Generating mrepo_c config")
	configPath, err := MrepoCConfig(repoPath, appstreamPath)
	if err != nil {
		return err
	}

	level.Debug(logger).Log("msg", "Modifying repo with mrepo_c", "configPath", *configPath)

	// log("Using mrepo_c config at", *configPath)
	repodataDir := path.Join(repoPath, "repodata")
	flags := []string{"-f", *configPath, repodataDir}

	if _, err := exec.Command("modifyrepo_c", flags...).Output(); err != nil {
		level.Error(logger).Log("msg", "modifyrepo_c failed", "error_msg", err)
		return err
	}

	level.Debug(logger).Log("msg", "Modified repo with mrepo_c successfully")
	defer func() {
		level.Debug(logger).Log("msg", "Config file contents", "contents", func() string {
					b, err := os.ReadFile(*configPath)
					if err != nil {
						return fmt.Sprintf("error reading config file: %v", err)
					}
					return string(b)
				}())
		level.Debug(logger).Log("msg", "Removing temporary mrepo_c config file", "configPath", *configPath)
		os.Remove(*configPath)

	}()
	return nil
}

func AddRpmToRepo(repoPath string, rpmFile io.ReadSeeker) error {
	info, err := GetRpmInfo(rpmFile)
	if err != nil {
		return err
	}

	file, err := os.Create(path.Join(repoPath, info.FileName))

	if err != nil {
		return err
	}

	defer file.Close()

	_, err = io.Copy(file, rpmFile)
	if err != nil {
		return err
	}

	return nil
}

func SignRepo(repoPath string, ring *pgp.KeyRing) error {
	file, err := os.Open(path.Join(repoPath, "repodata/repomd.xml"))
	if err != nil {
		return err
	}

	defer file.Close()

	sig, err := ring.SignDetachedStream(file)
	if err != nil {
		return err
	}

	armoredSig, err := sig.GetArmored()
	if err != nil {
		return err
	}

	if err := os.WriteFile(path.Join(repoPath, "repodata/repomd.xml.asc"), []byte(armoredSig), 0644); err != nil {
		return err
	}

	return nil
}

func SignRpmFile(rpmPath string, ring *pgp.KeyRing) error {
	key, err := ring.GetKey(0)
	if err != nil {
		return err
	}

	file, err := os.Open(rpmPath)
	if err != nil {
		return err
	}

	defer file.Close()

	if _, err := rpmutils.SignRpmFile(file, rpmPath, key.GetEntity().PrivateKey, nil); err != nil {
		return err
	}

	return nil
}
