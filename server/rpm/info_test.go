package rpm

import (
	"path/filepath"
	"testing"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/sassoftware/go-rpmutils"
)

func TestFileNameFromNEVRA(t *testing.T) {
	t.Parallel()

	nevra := rpmutils.NEVRA{
		Name:    "test-package",
		Epoch:   "1",
		Version: "2.3.4",
		Release: "5.fc42",
		Arch:    "x86_64",
	}

	if got, want := FileNameFromNEVRA(nevra, false), "test-package-1:2.3.4-5.fc42.x86_64.rpm"; got != want {
		t.Fatalf("FileNameFromNEVRA() = %q, want %q", got, want)
	}
}

func TestFileNameFromPartsSourceRPM(t *testing.T) {
	t.Parallel()

	got := FileNameFromParts("test-package", 0, "2.3.4", "5.fc42", "src")
	want := "test-package-0:2.3.4-5.fc42.src.rpm"

	if got != want {
		t.Fatalf("FileNameFromParts() = %q, want %q", got, want)
	}
}

func TestPackagePathUsesDeterministicPackageMetadata(t *testing.T) {
	t.Parallel()

	pkg := ent.RpmPackage{
		Name:     "test-package",
		Epoch:    1,
		Version:  "2.3.4",
		Release:  "5.fc42",
		Arch:     "aarch64",
		FilePath: "stale-or-wrong-file-name.rpm",
	}

	got := PackagePath(filepath.Join("var", "lib", "subatomic", "repo"), pkg)
	want := filepath.Join("var", "lib", "subatomic", "repo", "test-package-1:2.3.4-5.fc42.aarch64.rpm")

	if got != want {
		t.Fatalf("PackagePath() = %q, want %q", got, want)
	}
}
