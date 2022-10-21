package ostree

import (
	"io"
	"os"
	"os/exec"
)

func AddTarCommitToRepo(repo string, branch string, tar io.ReadSeeker) error {
	tmpFile, err := os.CreateTemp("", "ostree-upload")

	if err != nil {
		return err
	}

	defer tmpFile.Close()
	defer os.Remove(tmpFile.Name())
	println(tmpFile.Name())

	if _, err := io.Copy(tmpFile, tar); err != nil {
		return err
	}

	if _, err := exec.Command("ostree", "commit", "--branch", branch, "--tree=tar="+tmpFile.Name(), "--repo", repo).Output(); err != nil {
		return err
	}

	return nil
}

func UpdateSummary(repo string) error {
	if _, err := exec.Command("ostree", "summary", "-u", "--repo", repo).Output(); err != nil {
		return err
	}

	return nil
}
