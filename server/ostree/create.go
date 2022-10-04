package ostree

import (
	"github.com/ostreedev/ostree-go/pkg/otbuiltin"
)

func CreateRepo(path string) error {
	options := otbuiltin.NewInitOptions()
	options.Mode = "archive-z2"
	if _, err := otbuiltin.Init(path, options); err != nil {
		return err
	}

	return nil
}
