package cmd

import "github.com/spf13/cobra"

var keysCmd = &cobra.Command{
	Use:     "keys",
	Short:   "Manage keys",
	Aliases: []string{"key", "k", "gpg-keys", "gpg-key", "gpg"},
}

func init() {
	rootCmd.AddCommand(keysCmd)
}
