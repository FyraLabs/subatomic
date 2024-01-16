package cmd

import (
	"encoding/json"
	"fmt"
	"net/http"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var pkgSearchCmd = &cobra.Command{
	Use:     "search [repo] [query]",
	Short:   "Search for packages in a repository",
	Aliases: []string{"s"},
	Args:    cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		server := viper.GetString("server")
		token := viper.GetString("token")
		repo := args[0]
		query := args[1]
		// todo: Maybe add a flag to make them a table?

		req, err := http.NewRequest(http.MethodGet, server+"/repos/"+repo+"/rpms", nil)
		if err != nil {
			return err
		}

		req.Header.Add("Accept", "application/json")
		req.Header.Add("Authorization", "Bearer "+token)

		httpQuery := req.URL.Query()
		httpQuery.Add("name_contains", query)
		req.URL.RawQuery = httpQuery.Encode()

		client := &http.Client{}
		res, err := client.Do(req)
		if err != nil {
			return err
		}

		var result []types.RpmResponse

		if res.StatusCode != http.StatusOK {
			var serverError types.ErrResponse
			if err := json.NewDecoder(res.Body).Decode(&serverError); err != nil {
				return err
			}
		}

		// now decode the response into result

		if err := json.NewDecoder(res.Body).Decode(&result); err != nil {
			return err
		}

		// for result in results

		for _, result := range result {
			fmt.Println(result.FilePath)
		}

		return nil
	},
}

func init() {
	pkgCmd.AddCommand(pkgSearchCmd)
}
