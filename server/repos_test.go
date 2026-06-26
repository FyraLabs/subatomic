package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/FyraLabs/subatomic/server/rpm"
)

func TestResolveRpmPackagePathFallsBackToStoredFilePath(t *testing.T) {
	t.Parallel()

	repoPath := t.TempDir()
	pkg := ent.RpmPackage{
		Name:     "test-package",
		Epoch:    1,
		Version:  "2.3.4",
		Release:  "5.fc42",
		Arch:     "x86_64",
		FilePath: "legacy-file-name.rpm",
	}

	legacyPath := filepath.Join(repoPath, pkg.FilePath)
	if err := os.WriteFile(legacyPath, []byte("rpm"), 0644); err != nil {
		t.Fatalf("failed to write legacy test file: %v", err)
	}

	got, err := resolveRpmPackagePath(repoPath, pkg)
	if err != nil {
		t.Fatalf("resolveRpmPackagePath() returned error: %v", err)
	}

	if got != legacyPath {
		t.Fatalf("resolveRpmPackagePath() = %q, want %q", got, legacyPath)
	}
}

func TestResolveRpmPackagePathErrorsWithExpectedPaths(t *testing.T) {
	t.Parallel()

	repoPath := t.TempDir()
	pkg := ent.RpmPackage{
		Name:     "test-package",
		Epoch:    1,
		Version:  "2.3.4",
		Release:  "5.fc42",
		Arch:     "x86_64",
		FilePath: "legacy-file-name.rpm",
	}

	_, err := resolveRpmPackagePath(repoPath, pkg)
	if err == nil {
		t.Fatal("resolveRpmPackagePath() returned nil error, want missing-file error")
	}

	canonicalPath := rpm.PackagePath(repoPath, pkg)
	legacyPath := filepath.Join(repoPath, pkg.FilePath)
	errorText := err.Error()

	if !strings.Contains(errorText, "failed to find RPM package file") {
		t.Fatalf("error = %q, want missing package file message", errorText)
	}
	if !strings.Contains(errorText, canonicalPath) {
		t.Fatalf("error = %q, want canonical path %q", errorText, canonicalPath)
	}
	if !strings.Contains(errorText, legacyPath) {
		t.Fatalf("error = %q, want legacy path %q", errorText, legacyPath)
	}
}

func TestRemoveRpmPackageFileDeletesResolvedPath(t *testing.T) {
	t.Parallel()

	repoPath := t.TempDir()
	pkg := ent.RpmPackage{
		Name:    "test-package",
		Epoch:   1,
		Version: "2.3.4",
		Release: "5.fc42",
		Arch:    "x86_64",
	}

	packagePath := rpm.PackagePath(repoPath, pkg)
	if err := os.WriteFile(packagePath, []byte("rpm"), 0644); err != nil {
		t.Fatalf("failed to write test package file: %v", err)
	}

	deletedPath, err := removeRpmPackageFile(repoPath, pkg)
	if err != nil {
		t.Fatalf("removeRpmPackageFile() returned error: %v", err)
	}
	if deletedPath != packagePath {
		t.Fatalf("removeRpmPackageFile() = %q, want %q", deletedPath, packagePath)
	}
	if _, err := os.Stat(packagePath); !os.IsNotExist(err) {
		t.Fatalf("expected package file to be deleted, stat error: %v", err)
	}
}
