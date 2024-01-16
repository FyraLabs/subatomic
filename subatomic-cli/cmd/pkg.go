/*
Copyright © 2022 NAME HERE <EMAIL ADDRESS>
*/
package cmd

import "github.com/spf13/cobra"

var pkgCmd = &cobra.Command{
	Use:   "pkg [subcommand]",
	Short: "Manage packages on a Subatomic server",
	Aliases: []string{"package"},
	
}

func init() {
	rootCmd.AddCommand(pkgCmd)
}
