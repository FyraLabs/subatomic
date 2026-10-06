package rpm

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"strings"

	"github.com/FyraLabs/subatomic/server/logging"
	"github.com/FyraLabs/subatomic/server/tetsudou"
	"github.com/getsentry/sentry-go"
	"github.com/go-kit/log"
	"github.com/go-kit/log/level"

	pgp "github.com/ProtonMail/gopenpgp/v2/crypto"
	"github.com/sassoftware/go-rpmutils"
	"gopkg.in/ini.v1"
)

var logger = log.With(logging.Logger, "module", "rpm")

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

func UpdateRepo(repoPath string, ring *pgp.KeyRing) error {
	repoPath = filepath.Clean(repoPath)
	liveRepodata := path.Join(repoPath, "repodata")

	stagingPath, err := os.MkdirTemp("", "subatomic-"+path.Base(repoPath)+"-")
	if err != nil {
		return err
	}
	stagingRepodata := path.Join(stagingPath, "repodata")
	defer os.RemoveAll(stagingPath)

	_ = os.RemoveAll(path.Join(repoPath, ".repodata"))

	if exists, err := fileExists(path.Join(liveRepodata, "repomd.xml")); err != nil {
		return err
	} else if exists {
		if err := os.CopyFS(stagingRepodata, os.DirFS(liveRepodata)); err != nil {
			return err
		}

		if err := removeRepoAppStream(repoPath, stagingRepodata); err != nil {
			return err
		}
	}

	flags := []string{"--update", "--xz", "--local-sqlite", "--outputdir", stagingPath}

	if exists, err := fileExists(path.Join(repoPath, "comps.xml")); err != nil {
		return err
	} else if exists {
		flags = append(flags, "--groupfile", "comps.xml")
	}

	flags = append(flags, repoPath)

	level.Info(logger).Log("msg", "running createrepo_c", "flags", flags)
	if _, err := exec.Command("createrepo_c", flags...).Output(); err != nil {
		if err, ok := err.(*exec.ExitError); ok {
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

		modified, err := modifyRepoAppStream(repoPath, stagingRepodata, appstreamDir)
		if err != nil {
			reportAppStreamWarning("failed to modify repo appstream; continuing without refreshed appstream metadata", repoPath, err.Error())
		} else if modified {
			level.Info(logger).Log("msg", "modified repo appstream metadata successfully")
		}
	}

	if err := writeTetsudouMetadata(stagingPath); err != nil {
		return err
	}

	if ring != nil {
		if err := SignRepo(stagingPath, ring); err != nil {
			return err
		}
	}

	return publishRepodata(liveRepodata, stagingRepodata)
}

func publishRepodata(liveRepodata string, stagingRepodata string) error {
	if err := os.MkdirAll(liveRepodata, os.ModePerm); err != nil {
		return err
	}

	staged, err := os.ReadDir(stagingRepodata)
	if err != nil {
		return err
	}

	stagedNames := map[string]bool{}
	for _, entry := range staged {
		stagedNames[entry.Name()] = true
		if entry.IsDir() || entry.Name() == "repomd.xml" {
			continue
		}
		if err := copyFile(path.Join(stagingRepodata, entry.Name()), path.Join(liveRepodata, entry.Name())); err != nil {
			return err
		}
	}

	if err := copyFile(path.Join(stagingRepodata, "repomd.xml"), path.Join(liveRepodata, "repomd.xml")); err != nil {
		return err
	}

	live, err := os.ReadDir(liveRepodata)
	if err != nil {
		return err
	}
	for _, entry := range live {
		if stagedNames[entry.Name()] {
			continue
		}
		if err := os.RemoveAll(path.Join(liveRepodata, entry.Name())); err != nil {
			return err
		}
	}

	return nil
}

func copyFile(src string, dst string) error {
	srcFile, err := os.Open(src)
	if err != nil {
		return err
	}
	defer srcFile.Close()

	dstFile, err := os.Create(dst)
	if err != nil {
		return err
	}

	if _, err := io.Copy(dstFile, srcFile); err != nil {
		dstFile.Close()
		return err
	}

	return dstFile.Close()
}

func reportAppStreamWarning(message string, repoPath string, detail string) {
	level.Warn(logger).Log("msg", message, "repo", repoPath, "detail", detail)
	sentry.WithScope(func(scope *sentry.Scope) {
		scope.SetLevel(sentry.LevelWarning)
		scope.SetTag("module", "rpm")
		scope.SetContext("appstream", map[string]interface{}{
			"repo":   repoPath,
			"detail": detail,
		})
		sentry.CaptureMessage(message)
	})
}

func removeRepoAppStream(repoPath string, repodataDir string) error {
	if exists, err := fileExists(path.Join(repodataDir, "repomd.xml")); err != nil || !exists {
		return err
	}

	for _, metadataType := range []string{"appstream", "appstream_zck", "appstream-icons", "appstream-icons_zck"} {
		flags := []string{"--remove", metadataType, repodataDir}
		output, err := exec.Command("modifyrepo_c", flags...).CombinedOutput()
		if err != nil {
			return fmt.Errorf("modifyrepo_c returned non-zero exit code while removing %s with output %q: %w", metadataType, string(output), err)
		}
		if warning := strings.TrimSpace(string(output)); warning != "" && !strings.Contains(warning, "doesn't exist in repomd.xml") {
			reportAppStreamWarning("modifyrepo_c warned while removing appstream metadata", repoPath, warning)
		}
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

	ini.PrettyFormat = false
	inifile := ini.Empty()

	if exists, err := fileExists(iconsFile); err != nil {
		return nil, err
	} else if exists {
		section, err := inifile.NewSection("appstream-icons")
		if err != nil {
			return nil, err
		}
		if err := section.ReflectFrom(&iconsConfig); err != nil {
			return nil, err
		}
	}

	if exists, err := fileExists(appstreamFile); err != nil {
		return nil, err
	} else if exists {
		section, err := inifile.NewSection("appstream")
		if err != nil {
			return nil, err
		}
		if err := section.ReflectFrom(&appstreamConfig); err != nil {
			return nil, err
		}
	}

	configFileName := fmt.Sprintf("%s-mrepoc.ini", repoName)
	configPath := path.Join("/tmp", configFileName)
	if err := inifile.SaveTo(configPath); err != nil {
		return nil, err
	}
	level.Debug(logger).Log("msg", "Generated mrepo_c config", "configPath", configPath)

	return &configPath, nil
}

func fileExists(filePath string) (bool, error) {
	_, err := os.Stat(filePath)
	if os.IsNotExist(err) {
		return false, nil
	}
	return err == nil, err
}

func appStreamMetadataPaths(repoPath string, appstreamPath string) []string {
	repoName := path.Base(repoPath)
	appstreamDir := path.Join(appstreamPath, repoName, "latest/appstream")
	return []string{
		path.Join(appstreamDir, fmt.Sprintf("%s.xml.gz", repoName)),
		path.Join(appstreamDir, fmt.Sprintf("%s-icons-64x64.tar.gz", repoName)),
	}
}

func ModifyRepoAppStream(repoPath string, appstreamPath string) error {
	_, err := modifyRepoAppStream(repoPath, path.Join(repoPath, "repodata"), appstreamPath)
	return err
}

func modifyRepoAppStream(repoPath string, repodataDir string, appstreamPath string) (bool, error) {
	paths := appStreamMetadataPaths(repoPath, appstreamPath)
	metadata := []struct {
		filePath string
		dataType string
		newName  string
	}{
		{filePath: paths[0], dataType: "appstream", newName: "appstream.xml"},
		{filePath: paths[1], dataType: "appstream-icons", newName: "appstream-icons-64x64.tar"},
	}

	modified := false
	var missingMetadata []string
	for _, item := range metadata {
		exists, err := fileExists(item.filePath)
		if err != nil {
			return false, err
		}
		if !exists {
			missingMetadata = append(missingMetadata, item.filePath)
			continue
		}

		flags := []string{
			"--mdtype", item.dataType,
			"--new-name", item.newName,
			item.filePath,
			repodataDir,
		}
		level.Debug(logger).Log("msg", "Modifying repo with mrepo_c", "metadata_path", item.filePath)
		output, err := exec.Command("modifyrepo_c", flags...).CombinedOutput()
		if err != nil {
			return modified, fmt.Errorf("modifyrepo_c returned non-zero exit code while adding %s with output %q: %w", item.dataType, string(output), err)
		}
		if warning := strings.TrimSpace(string(output)); warning != "" {
			reportAppStreamWarning("modifyrepo_c warned while adding appstream metadata", repoPath, warning)
		}
		modified = true
	}

	if len(missingMetadata) > 0 {
		reportAppStreamWarning("appstream source metadata does not exist; skipping missing files", repoPath, strings.Join(missingMetadata, ","))
	}
	return modified, nil
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
