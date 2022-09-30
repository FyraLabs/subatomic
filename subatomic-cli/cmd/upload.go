/*
Copyright © 2022 Fyra Labs
*/
package cmd

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
	"os"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
	"golang.org/x/sync/errgroup"
)

// uploadCmd represents the upload command
var uploadCmd = &cobra.Command{
	Use:   "upload [repo_id] [files...]",
	Short: "Upload artifacts to a repository",
	Args:  cobra.MinimumNArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		server := viper.GetString("server")
		token := viper.GetString("token")

		if server == "" {
			return errors.New("server must be defined")
		}

		if token == "" {
			return errors.New("token must be defined")
		}

		repoID := args[0]

		pipeReader, pipeWriter := io.Pipe()
		form := multipart.NewWriter(pipeWriter)

		req, err := http.NewRequest(http.MethodPut, server+"/api/repos/"+repoID, pipeReader)
		if err != nil {
			return err
		}

		req.Header.Add("Content-Type", form.FormDataContentType())
		req.Header.Add("Accept", "application/json")
		req.Header.Add("Authorization", "Bearer "+token)

		g := new(errgroup.Group)

		g.Go(func() error {
			defer pipeWriter.Close()
			defer form.Close()

			for _, filename := range args[1:] {
				file, err := os.Open(filename)
				if err != nil {
					return err
				}

				defer file.Close()

				formWriter, err := form.CreateFormFile("file_upload", file.Name())
				if err != nil {
					return err
				}

				if _, err := io.Copy(formWriter, file); err != nil {
					return err
				}
			}

			return nil
		})

		var res *http.Response
		defer func() {
			if res != nil {
				res.Body.Close()
			}
		}()

		g.Go(func() error {
			client := &http.Client{}
			res, err = client.Do(req)
			if err != nil {
				return err
			}
			return nil
		})

		if err := g.Wait(); err != nil {
			return err
		}

		if res.StatusCode != http.StatusNoContent {
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
	rootCmd.AddCommand(uploadCmd)

	// Here you will define your flags and configuration settings.

	// Cobra supports Persistent Flags which will work for this command
	// and all subcommands, e.g.:
	// uploadCmd.PersistentFlags().String("foo", "", "A help for foo")

	// Cobra supports local flags which will only run when this command
	// is called directly, e.g.:
	// uploadCmd.Flags().BoolP("toggle", "t", false, "Help message for toggle")
}