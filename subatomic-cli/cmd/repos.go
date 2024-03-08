package cmd

import (
	"github.com/spf13/cobra"
)

// reposCmd represents the repos command
var repoCmd = &cobra.Command{
	Use:   "repo",
	Short: "Manage repo on a Subatomic server",
}

func init() {
	rootCmd.AddCommand(repoCmd)

	// Here you will define your flags and configuration settings.

	// Cobra supports Persistent Flags which will work for this command
	// and all subcommands, e.g.:
	// reposCmd.PersistentFlags().String("foo", "", "A help for foo")

	// Cobra supports local flags which will only run when this command
	// is called directly, e.g.:
	// reposCmd.Flags().BoolP("toggle", "t", false, "Help message for toggle")
}
