package tetsudou

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"time"

	"github.com/FyraLabs/subatomic/server/logging"
	"github.com/cenkalti/backoff/v4"
	"github.com/go-kit/log"
	"github.com/go-kit/log/level"
)

var logger = log.With(logging.Logger, "module", "tetsudou")

type TetsudouConfig struct {
	Server string
	Token  string
}

func RefreshRepo(config *TetsudouConfig, repoid string, repodata *Repodata) error {
	path, err := url.JoinPath(config.Server, "/api/repos/"+repoid)
	if err != nil {
		return err
	}

	payload, err := json.Marshal(repodata)
	if err != nil {
		return err
	}

	client := &http.Client{Timeout: 30 * time.Second}

	// Ideally this should never happen, given the reliability of Cloudflare.
	// However, shit happens. We won't block too long though since we're actively holding a lock on the repo.
	bo := backoff.NewExponentialBackOff(backoff.WithMaxElapsedTime(time.Minute))

	return backoff.RetryNotify(func() error {
		req, err := http.NewRequest(http.MethodPost, path, bytes.NewReader(payload))
		if err != nil {
			return backoff.Permanent(err)
		}
		req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", config.Token))
		req.Header.Set("Content-Type", "application/json")

		resp, err := client.Do(req)
		if err != nil {
			return err
		}
		defer resp.Body.Close()

		if resp.StatusCode != http.StatusNoContent {
			return fmt.Errorf("unexpected status code: %d", resp.StatusCode)
		}

		return nil
	}, bo, func(err error, d time.Duration) {
		level.Warn(logger).Log("msg", "retrying tetsudou refresh", "repo_id", repoid, "in", d, "error", err)
	})
}

func DeleteRepo(config *TetsudouConfig, repoid string) error {
	path, err := url.JoinPath(config.Server, "/api/repos/"+repoid)
	if err != nil {
		return err
	}

	req, err := http.NewRequest(http.MethodDelete, path, nil)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", config.Token))

	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusNoContent {
		return fmt.Errorf("unexpected status code: %d", resp.StatusCode)
	}

	return nil
}
