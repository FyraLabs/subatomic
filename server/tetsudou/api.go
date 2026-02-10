package tetsudou

import (
	"fmt"
	"net/http"
	"net/url"
	"time"
)

type TetsudouConfig struct {
	Server string
	Token  string
}

func RefreshRepo(config *TetsudouConfig, repoid string) error {
	path, err := url.JoinPath(config.Server, "/api/repos/"+repoid+"/refresh")
	if err != nil {
		return err
	}

	req, err := http.NewRequest(http.MethodPost, path, nil)
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
