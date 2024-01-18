package cmd

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"

	// "github.com/FyraLabs/subatomic/server/types"
	"github.com/FyraLabs/subatomic/server/types"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var keysListCmd = &cobra.Command{
	Use:     "list",
	Short:   "List keys",
	Aliases: []string{"ls", "l"},

	RunE: func(cmd *cobra.Command, args []string) error {
		server := viper.GetString("server")
		token := viper.GetString("token")
		if server == "" {
			return errors.New("server must be defined")
		}

		if token == "" {
			return errors.New("token must be defined")
		}

		req, err := http.NewRequest(http.MethodGet, server+"/keys", nil)
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
		var result []types.KeyResponse // why is not a type?

		if res.StatusCode != http.StatusOK {
			var serverError types.ErrResponse
			if err := json.NewDecoder(res.Body).Decode(&serverError); err != nil {
				return err
			}
		}

		if err := json.NewDecoder(res.Body).Decode(&result); err != nil {
			return err
		}

		// let's make it TSV because it's funny
		println("ID\tName\tEmail")

		for _, result := range result {
			result := fmt.Sprintf("%s\t%s\t%s", result.ID, result.Name, result.Email)
			fmt.Println(result)
		}

		return nil
	},
}

func init() {
	keysCmd.AddCommand(keysListCmd)
}
