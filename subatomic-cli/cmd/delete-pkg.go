package cmd

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strconv"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

func resolvePackageNevra(server string, token string, repo string, input string) ([]types.RpmResponse, error) {
	req, err := http.NewRequest(http.MethodGet, server+"/repos/"+repo+"/rpms", nil)
	if err != nil {
		return nil, err
	}

	req.Header.Add("Accept", "application/json")
	req.Header.Add("Authorization", "Bearer "+token)

	query := req.URL.Query()
	query.Add("file_path", input)
	req.URL.RawQuery = query.Encode()

	client := &http.Client{}
	res, err := client.Do(req)
	if err != nil {
		return nil, err
	}

	print(res.StatusCode)

	if res.StatusCode != http.StatusOK {
		var serverError types.ErrResponse
		if err := json.NewDecoder(res.Body).Decode(&serverError); err != nil {
			return nil, err
		}

		return nil, fmt.Errorf("API returned error: %s", serverError.ErrorText)
	}

	var result []types.RpmResponse

	if err := json.NewDecoder(res.Body).Decode(&result); err != nil {
		return nil, err
	}

	return result, nil

}

var pkgDeleteCmd = &cobra.Command{
	Use:   "delete [repo] [id or spec]",
	Short: "Delete a package",
	RunE: func(cmd *cobra.Command, args []string) error {
		server := viper.GetString("server")
		token := viper.GetString("token")

		if server == "" {
			return errors.New("server must be defined")
		}

		if token == "" {
			return errors.New("token must be defined")
		}

		repo := args[0]

		if repo == "" {
			return errors.New("repo must be defined")
		}

		input := args[1]

		if input == "" {
			return errors.New("input must be defined")
		}

		// check if input is an integer or string

		var pkgId int

		// check if can be converted into int

		if out, err := strconv.Atoi(input); err == nil {
			// is int
			pkgId = out
		} else {
			// query for package with filename

			query, err := resolvePackageNevra(server, token, repo, input)
			if err != nil {
				return err
			}

			if len(query) == 0 {
				return errors.New("no packages found")
			}

			pkgId = query[0].ID
		}

		req, err := http.NewRequest(http.MethodDelete, server+"/repos/"+repo+"/rpms/"+strconv.Itoa(pkgId), nil)
		if err != nil {
			return err
		}

		req.Header.Add("Accept", "application/json")
		req.Header.Add("Authorization", "Bearer "+token)

		client := &http.Client{}
		res, err := client.Do(req)

		if err != nil {
			return err
		}

		// if status code is not 204, return error
		if res.StatusCode != http.StatusNoContent {
			var serverError types.ErrResponse
			if err := json.NewDecoder(res.Body).Decode(&serverError); err != nil {
				return err
			}
		}
		return nil
	},
}

func init() {
	rootCmd.AddCommand(pkgDeleteCmd)
}
