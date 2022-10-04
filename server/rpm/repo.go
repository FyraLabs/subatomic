package rpm

import (
	"io"
	"os"
	"os/exec"
)

func CreateRepo(path string) error {
	if err := os.MkdirAll(path, os.ModePerm); err != nil {
		return nil
	}

	if _, err := exec.Command("createrepo_c", path).Output(); err != nil {
		return err
	}

	return nil
}

func UpdateRepo(path string) error {
	if _, err := exec.Command("createrepo_c", "--update", "--deltas", "--zck", "--xz", path).Output(); err != nil {
		return err
	}

	return nil
}

func AddRpmToRepo(path string, rpmFile io.ReadSeeker) error {
	file, err := os.Create(path)

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
