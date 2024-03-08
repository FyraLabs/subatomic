/*
Copyright © 2022 NAME HERE <EMAIL ADDRESS>
*/
package cmd

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

// repoCreateCmd represents the create command
var repoCreateCmd = &cobra.Command{
	Use:   "create [id] [type]",
	Short: "Create a new repo",
	Args:  cobra.ExactArgs(2),

	RunE: func(cmd *cobra.Command, args []string) error {
		server := viper.GetString("server")
		token := viper.GetString("token")

		if server == "" {
			return errors.New("server must be defined")
		}

		if token == "" {
			return errors.New("token must be defined")
		}

		payload := types.CreateRepoPayload{
			ID:       args[0],
			RepoType: args[1],
		}

		data, err := json.Marshal(payload)
		if err != nil {
			return err
		}

		req, err := http.NewRequest(http.MethodPost, server+"/repos", bytes.NewReader(data))
		if err != nil {
			return err
		}

		req.Header.Set("Content-Type", "application/json")
		req.Header.Add("Accept", "application/json")
		req.Header.Add("Authorization", "Bearer "+token)

		client := &http.Client{}
		res, err := client.Do(req)
		if err != nil {
			return err
		}

		if res.StatusCode != http.StatusCreated {
			var serverError types.ErrResponse
			if err := json.NewDecoder(res.Body).Decode(&serverError); err != nil {
				return err
			}

			return fmt.Errorf("API returned error: %s", serverError.ErrorText)
		}

		return nil
	},
}

func init() {
	repoCmd.AddCommand(repoCreateCmd)

	// Here you will define your flags and configuration settings.

	// Cobra supports Persistent Flags which will work for this command
	// and all subcommands, e.g.:
	// repoCreateCmd.PersistentFlags().String("foo", "", "A help for foo")

	// Cobra supports local flags which will only run when this command
	// is called directly, e.g.:
	// repoCreateCmd.Flags().BoolP("toggle", "t", false, "Help message for toggle")
}
