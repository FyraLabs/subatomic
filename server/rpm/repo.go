package rpm

import (
	"io"
	"os"
	"os/exec"
	"path"

	pgp "github.com/ProtonMail/gopenpgp/v2/crypto"
	"github.com/sassoftware/go-rpmutils"
)

func CreateRepo(repoPath string) error {
	if err := os.MkdirAll(repoPath, os.ModePerm); err != nil {
		return nil
	}

	if _, err := exec.Command("createrepo_c", repoPath).Output(); err != nil {
		return err
	}

	return nil
}

func UpdateRepo(repoPath string) error {
	flags := []string{"--update", "--deltas", "--zck", "--xz"}

	_, err := os.Stat(path.Join(repoPath, "comps.xml"))
	if err != nil && !os.IsNotExist(err) {
		return err
	}

	if err == nil {
		flags = append(flags, "--groupfile", "comps.xml")
	}

	flags = append(flags, repoPath)

	if _, err := exec.Command("createrepo_c", flags...).Output(); err != nil {
		return err
	}

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

	if err := os.WriteFile(path.Join(repoPath, "repodata/repomd.xml.asc"), []byte(armoredSig), 0744); err != nil {
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
