package cmd

import (
	"errors"

	// "github.com/FyraLabs/subatomic/server/types"
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

		// var result []types.keyResponse // why is not a type?


		return nil
	},
}

func init() {
	keysCmd.AddCommand(keysListCmd)
}
