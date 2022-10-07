package rpm

import (
	"fmt"
	"io"
	"strconv"
	"strings"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/samber/lo"
	"github.com/sassoftware/go-rpmutils"
)

type RpmInfo struct {
	Name     string
	Epoch    int
	Version  string
	Release  string
	Arch     string
	IsSource bool
	FileName string
	*rpmutils.NEVRA
	*rpmutils.Rpm
}

func GetRpmInfo(file io.ReadSeeker) (*RpmInfo, error) {
	rpmPackage, err := rpmutils.ReadRpm(file)
	if err != nil {
		return nil, fmt.Errorf("failed to read RPM: %w", err)
	}

	nevra, err := rpmPackage.Header.GetNEVRA()
	if err != nil {
		return nil, fmt.Errorf("failed to get rpm NEVRA: %w", err)
	}

	epoch, err := strconv.Atoi(nevra.Epoch)
	if err != nil {
		// errors.
		return nil, fmt.Errorf("failed to convert epoch to int: %w", err)
	}

	isSource := !rpmPackage.Header.HasTag(rpmutils.SOURCERPM)

	if _, err := file.Seek(0, io.SeekStart); err != nil {
		return nil, err
	}

	fileName := strings.TrimSuffix(nevra.String(), ".rpm") + lo.Ternary(isSource, ".src.rpm", ".rpm")

	return &RpmInfo{
		Name:     nevra.Name,
		Epoch:    epoch,
		Version:  nevra.Version,
		Release:  nevra.Release,
		Arch:     lo.Ternary(isSource, "src", nevra.Arch),
		IsSource: isSource,
		FileName: fileName,
		Rpm:      rpmPackage,
		NEVRA:    nevra,
	}, nil
}

func DBPackageToNEVRA(pkg ent.RpmPackage) rpmutils.NEVRA {
	return rpmutils.NEVRA{
		Name:    pkg.Name,
		Epoch:   strconv.Itoa(pkg.Epoch),
		Version: pkg.Version,
		Release: pkg.Release,
		Arch:    pkg.Arch,
	}
}
