package rpm

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"gopkg.in/ini.v1"
)

func TestUpdateRepoRepairsMissingAppStreamPayloadForSourceRepo(t *testing.T) {
	if _, err := exec.LookPath("createrepo_c"); err != nil {
		t.Skip("createrepo_c is not installed")
	}
	if _, err := exec.LookPath("modifyrepo_c"); err != nil {
		t.Skip("modifyrepo_c is not installed")
	}

	repoPath := filepath.Join(t.TempDir(), "terrarawhide-source")
	if err := os.Mkdir(repoPath, 0o755); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command("createrepo_c", repoPath).CombinedOutput(); err != nil {
		t.Fatalf("createrepo_c failed: %s: %v", output, err)
	}

	iconsPath := filepath.Join(t.TempDir(), "icons.tar")
	if err := os.WriteFile(iconsPath, []byte("icons"), 0o644); err != nil {
		t.Fatal(err)
	}
	repodataPath := filepath.Join(repoPath, "repodata")
	if output, err := exec.Command(
		"modifyrepo_c",
		"--mdtype", "appstream-icons",
		"--new-name", "appstream-icons-64x64.tar",
		"--compress-type", "zck",
		iconsPath,
		repodataPath,
	).CombinedOutput(); err != nil {
		t.Fatalf("modifyrepo_c failed: %s: %v", output, err)
	}

	entries, err := os.ReadDir(repodataPath)
	if err != nil {
		t.Fatal(err)
	}
	removedPayload := false
	for _, entry := range entries {
		if strings.HasSuffix(entry.Name(), "-appstream-icons-64x64.tar.zck") {
			if err := os.Remove(filepath.Join(repodataPath, entry.Name())); err != nil {
				t.Fatal(err)
			}
			removedPayload = true
		}
	}
	if !removedPayload {
		t.Fatal("modifyrepo_c did not create an AppStream icon payload")
	}

	t.Setenv("SUBATOMIC_APPSTREAM_DIR", t.TempDir())
	if err := UpdateRepo(repoPath); err != nil {
		t.Fatalf("UpdateRepo() failed: %v", err)
	}

	repomd, err := os.ReadFile(filepath.Join(repodataPath, "repomd.xml"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(repomd), `type="appstream-icons"`) {
		t.Fatal("appstream-icons entry was not removed")
	}
	if _, err := os.Stat(filepath.Join(repodataPath, "tetsudou.json")); err != nil {
		t.Fatalf("expected complete repository update to write tetsudou metadata: %v", err)
	}
}

func TestMrepoCConfigOnlyIncludesExistingMetadata(t *testing.T) {
	repoPath := filepath.Join(t.TempDir(), "test-repo")
	appstreamPath := t.TempDir()
	metadataDir := filepath.Join(appstreamPath, "test-repo", "latest", "appstream")
	if err := os.MkdirAll(metadataDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(metadataDir, "test-repo.xml.gz"), nil, 0o644); err != nil {
		t.Fatal(err)
	}

	configPath, err := MrepoCConfig(repoPath, appstreamPath)
	if err != nil {
		t.Fatal(err)
	}
	defer os.Remove(*configPath)

	config, err := ini.Load(*configPath)
	if err != nil {
		t.Fatal(err)
	}
	if !config.HasSection("appstream") {
		t.Fatal("expected appstream section")
	}
	if got := config.Section("appstream").Key("path").String(); got != filepath.Join(metadataDir, "test-repo.xml.gz") {
		t.Fatalf("appstream path = %q", got)
	}
	if config.HasSection("appstream-icons") {
		t.Fatal("did not expect appstream-icons section without an icons archive")
	}
}

func TestModifyRepoAppStreamSkipsMissingMetadata(t *testing.T) {
	modified, err := modifyRepoAppStream(filepath.Join(t.TempDir(), "source-repo"), t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if modified {
		t.Fatal("expected missing AppStream metadata to be skipped")
	}
}
