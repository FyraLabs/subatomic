package rpm

import (
	"fmt"
	"io"
	"path/filepath"
	"strconv"

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

	fileName := FileNameFromNEVRA(*nevra, isSource)

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

func FileNameFromNEVRA(nevra rpmutils.NEVRA, isSource bool) string {
	arch := lo.Ternary(isSource, "src", nevra.Arch)
	return fmt.Sprintf("%s-%s:%s-%s.%s.rpm", nevra.Name, nevra.Epoch, nevra.Version, nevra.Release, arch)
}

func FileNameFromParts(name string, epoch int, version string, release string, arch string) string {
	return FileNameFromNEVRA(rpmutils.NEVRA{
		Name:    name,
		Epoch:   strconv.Itoa(epoch),
		Version: version,
		Release: release,
		Arch:    arch,
	}, arch == "src")
}

func FileNameFromPackage(pkg ent.RpmPackage) string {
	return FileNameFromParts(pkg.Name, pkg.Epoch, pkg.Version, pkg.Release, pkg.Arch)
}

func PackagePath(repoPath string, pkg ent.RpmPackage) string {
	return filepath.Join(repoPath, FileNameFromPackage(pkg))
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
