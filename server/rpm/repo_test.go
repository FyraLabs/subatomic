package rpm

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/FyraLabs/subatomic/server/tetsudou"
	pgp "github.com/ProtonMail/gopenpgp/v2/crypto"
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
	// Production repos also carry _zck records, which are added by modifyrepo_c --zck
	if output, err := exec.Command(
		"modifyrepo_c",
		"--zck",
		"--mdtype", "appstream",
		"--new-name", "appstream.xml",
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

	key, err := pgp.GenerateKey("Test", "test@example.com", "x25519", 0)
	if err != nil {
		t.Fatal(err)
	}
	ring, err := pgp.NewKeyRing(key)
	if err != nil {
		t.Fatal(err)
	}

	var repodata *tetsudou.Repodata
	var repomdWhenRefreshed []byte
	tetsudouServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if want := "/api/repos/terrarawhide-source"; r.URL.Path != want {
			t.Errorf("tetsudou refreshed %s, want %s", r.URL.Path, want)
		}
		repodata = &tetsudou.Repodata{}
		if err := json.NewDecoder(r.Body).Decode(repodata); err != nil {
			t.Error(err)
		}
		// The refresh has to happen before the swap, so repomd.xml is still the old one
		repomdWhenRefreshed, err = os.ReadFile(filepath.Join(repodataPath, "repomd.xml"))
		if err != nil {
			t.Error(err)
		}
		w.WriteHeader(http.StatusNoContent)
	}))
	defer tetsudouServer.Close()

	t.Setenv("SUBATOMIC_APPSTREAM_DIR", t.TempDir())
	if err := UpdateRepo(repoPath, ring, "terrarawhide-source", &tetsudou.TetsudouConfig{Server: tetsudouServer.URL}); err != nil {
		t.Fatalf("UpdateRepo() failed: %v", err)
	}
	if repodata == nil {
		t.Fatal("UpdateRepo() did not refresh tetsudou")
	}

	repomd, err := os.ReadFile(filepath.Join(repodataPath, "repomd.xml"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(repomd), `type="appstream`) {
		t.Fatalf("appstream entries were not removed:\n%s", repomd)
	}

	assertNotExists(t, filepath.Join(repodataPath, "tetsudou.json"))
	hashes, err := tetsudou.HashesFromReader(strings.NewReader(string(repomd)))
	if err != nil {
		t.Fatal(err)
	}
	if repodata.Hashes != hashes || repodata.Size != int64(len(repomd)) {
		t.Fatalf("refreshed repodata does not describe repomd.xml: got %+v, want %+v", repodata.Hashes, hashes)
	}
	if string(repomdWhenRefreshed) == string(repomd) {
		t.Fatal("tetsudou was refreshed after repomd.xml had already been swapped in")
	}

	armoredSig, err := os.ReadFile(filepath.Join(repodataPath, "repomd.xml.asc"))
	if err != nil {
		t.Fatal(err)
	}
	sig, err := pgp.NewPGPSignatureFromArmored(string(armoredSig))
	if err != nil {
		t.Fatal(err)
	}
	if err := ring.VerifyDetached(pgp.NewPlainMessage(repomd), sig, pgp.GetUnixTime()); err != nil {
		t.Fatalf("repomd.xml.asc does not verify against repomd.xml: %v", err)
	}

	for _, leftover := range []string{
		filepath.Join(repoPath, ".repodata"),
		filepath.Join(filepath.Dir(repoPath), ".terrarawhide-source.staging"),
	} {
		if _, err := os.Stat(leftover); !os.IsNotExist(err) {
			t.Fatalf("expected %s to be cleaned up: %v", leftover, err)
		}
	}
}

func TestUpdateRepoLeavesRepodataUntouchedOnFailure(t *testing.T) {
	if _, err := exec.LookPath("createrepo_c"); err != nil {
		t.Skip("createrepo_c is not installed")
	}
	if _, err := exec.LookPath("modifyrepo_c"); err != nil {
		t.Skip("modifyrepo_c is not installed")
	}

	repoPath := filepath.Join(t.TempDir(), "test-repo")
	if err := os.Mkdir(repoPath, 0o755); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command("createrepo_c", repoPath).CombinedOutput(); err != nil {
		t.Fatalf("createrepo_c failed: %s: %v", output, err)
	}
	repomdPath := filepath.Join(repoPath, "repodata", "repomd.xml")
	before, err := os.ReadFile(repomdPath)
	if err != nil {
		t.Fatal(err)
	}

	// An invalid groupfile makes createrepo_c fail after the existing metadata has been loaded
	if err := os.WriteFile(filepath.Join(repoPath, "comps.xml"), []byte("not xml"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := UpdateRepo(repoPath, nil, "test-repo", nil); err == nil {
		t.Fatal("expected UpdateRepo() to fail with an invalid groupfile")
	}

	after, err := os.ReadFile(repomdPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(before) != string(after) {
		t.Fatal("failed update modified the live repomd.xml")
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
	repoPath := filepath.Join(t.TempDir(), "source-repo")
	modified, err := modifyRepoAppStream(repoPath, filepath.Join(repoPath, "repodata"), t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if modified {
		t.Fatal("expected missing AppStream metadata to be skipped")
	}
}

func writeTestRepodata(t *testing.T, dir string, files map[string]string) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}

	repomd := "<repomd>"
	for name, content := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
		repomd += `<data type="` + name + `"><location href="repodata/` + name + `"/></data>`
	}
	repomd += "</repomd>"

	if err := os.WriteFile(filepath.Join(dir, "repomd.xml"), []byte(repomd), 0o644); err != nil {
		t.Fatal(err)
	}
}

func assertFileContent(t *testing.T, filePath string, want string) {
	t.Helper()
	got, err := os.ReadFile(filePath)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != want {
		t.Fatalf("%s = %q, want %q", filePath, got, want)
	}
}

func assertNotExists(t *testing.T, filePath string) {
	t.Helper()
	if _, err := os.Stat(filePath); !os.IsNotExist(err) {
		t.Fatalf("expected %s to not exist: %v", filePath, err)
	}
}

func TestPublishRepodataReplacesMetadataInPlace(t *testing.T) {
	liveRepodata := filepath.Join(t.TempDir(), "repodata")
	writeTestRepodata(t, liveRepodata, map[string]string{"aaa-primary.xml.xz": "old primary", "ccc-other.xml.xz": "other"})
	if err := os.WriteFile(filepath.Join(liveRepodata, "repomd.xml.asc"), []byte("old signature"), 0o644); err != nil {
		t.Fatal(err)
	}

	stagingRepodata := filepath.Join(t.TempDir(), "repodata")
	writeTestRepodata(t, stagingRepodata, map[string]string{"bbb-primary.xml.xz": "new primary", "ccc-other.xml.xz": "other"})
	stagedRepomd, err := os.ReadFile(filepath.Join(stagingRepodata, "repomd.xml"))
	if err != nil {
		t.Fatal(err)
	}

	if err := publishRepodata(liveRepodata, stagingRepodata, "test-repo", nil, nil); err != nil {
		t.Fatal(err)
	}

	assertFileContent(t, filepath.Join(liveRepodata, "repomd.xml"), string(stagedRepomd))
	assertFileContent(t, filepath.Join(liveRepodata, "bbb-primary.xml.xz"), "new primary")
	assertFileContent(t, filepath.Join(liveRepodata, "ccc-other.xml.xz"), "other")
	// Clients holding the previous repomd.xml must still be able to fetch what it references
	assertFileContent(t, filepath.Join(liveRepodata, "aaa-primary.xml.xz"), "old primary")
	// The update was unsigned, so the old signature no longer matches repomd.xml
	assertNotExists(t, filepath.Join(liveRepodata, "repomd.xml.asc"))
}

func TestPublishRepodataRemovesRetiredMetadataAfterGracePeriod(t *testing.T) {
	liveRepodata := filepath.Join(t.TempDir(), "repodata")
	writeTestRepodata(t, liveRepodata, map[string]string{"aaa-primary.xml.xz": "first"})

	stagingRepodata := filepath.Join(t.TempDir(), "repodata")
	writeTestRepodata(t, stagingRepodata, map[string]string{"bbb-primary.xml.xz": "second"})
	if err := publishRepodata(liveRepodata, stagingRepodata, "test-repo", nil, nil); err != nil {
		t.Fatal(err)
	}
	assertFileContent(t, filepath.Join(liveRepodata, "aaa-primary.xml.xz"), "first")

	// Retired files must not be carried into the next generation's staging directory
	seeded := filepath.Join(t.TempDir(), "repodata")
	if err := seedRepodata(liveRepodata, seeded); err != nil {
		t.Fatal(err)
	}
	assertNotExists(t, filepath.Join(seeded, "aaa-primary.xml.xz"))
	assertFileContent(t, filepath.Join(seeded, "bbb-primary.xml.xz"), "second")

	// Once the grace period has passed since aaa was retired, it is removed, while bbb was only just retired and is kept
	if err := os.WriteFile(filepath.Join(liveRepodata, "ccc-primary.xml.xz"), []byte("third"), 0o644); err != nil {
		t.Fatal(err)
	}
	stagedNames := map[string]bool{"repomd.xml": true, "ccc-primary.xml.xz": true}
	if err := pruneRetiredRepodata(liveRepodata, stagedNames, time.Now().Add(retiredRepodataGracePeriod)); err != nil {
		t.Fatal(err)
	}
	assertNotExists(t, filepath.Join(liveRepodata, "aaa-primary.xml.xz"))
	assertFileContent(t, filepath.Join(liveRepodata, "bbb-primary.xml.xz"), "second")

	if err := pruneRetiredRepodata(liveRepodata, stagedNames, time.Now().Add(2*retiredRepodataGracePeriod)); err != nil {
		t.Fatal(err)
	}
	assertNotExists(t, filepath.Join(liveRepodata, "bbb-primary.xml.xz"))
	assertNotExists(t, filepath.Join(filepath.Dir(liveRepodata), retiredRepodataStateFile))
}
