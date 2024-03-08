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

// keyCreateCmd represents the create command
var keyCreateCmd = &cobra.Command{
	Use:   "create [id] [name] [email]",
	Short: "Create a new key",
	Args:  cobra.ExactArgs(3),

	RunE: func(cmd *cobra.Command, args []string) error {
		server := viper.GetString("server")
		token := viper.GetString("token")

		if server == "" {
			return errors.New("server must be defined")
		}

		if token == "" {
			return errors.New("token must be defined")
		}

		payload := types.CreateKeyPayload{
			ID:    args[0],
			Name:  args[1],
			Email: args[2],
		}

		data, err := json.Marshal(payload)
		if err != nil {
			return err
		}

		req, err := http.NewRequest(http.MethodPost, server+"/keys", bytes.NewReader(data))
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
	keysCmd.AddCommand(keyCreateCmd)

	// Here you will define your flags and configuration settings.

	// Cobra supports Persistent Flags which will work for this command
	// and all subcommands, e.g.:
	// keyCreateCmd.PersistentFlags().String("foo", "", "A help for foo")

	// Cobra supports local flags which will only run when this command
	// is called directly, e.g.:
	// keyCreateCmd.Flags().BoolP("toggle", "t", false, "Help message for toggle")
}
