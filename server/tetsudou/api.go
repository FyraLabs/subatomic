package tetsudou

import (
	"fmt"
	"net/http"
	"net/url"
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

	resp, err := http.DefaultClient.Do(req)
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

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusNoContent {
		return fmt.Errorf("unexpected status code: %d", resp.StatusCode)
	}

	return nil
}
