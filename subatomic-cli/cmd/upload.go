package cmd

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"mime/multipart"
	"net/http"
	"os"
	"time"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/cenkalti/backoff/v4"
	"github.com/samber/lo"
	"github.com/schollz/progressbar/v3"
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
		prune, err := cmd.Flags().GetBool("prune")
		if err != nil {
			return err
		}

		server := viper.GetString("server")
		token := viper.GetString("token")

		if server == "" {
			return errors.New("server must be defined")
		}

		if token == "" {
			return errors.New("token must be defined")
		}

		repoID := args[0]

		return backoff.RetryNotify(func() error {
			pipeReader, pipeWriter := io.Pipe()
			form := multipart.NewWriter(pipeWriter)

			req, err := http.NewRequest(http.MethodPut, server+"/repos/"+repoID, pipeReader)
			if err != nil {
				return err
			}

			q := req.URL.Query()
			q.Add("prune", lo.Ternary(prune, "true", "false"))
			req.URL.RawQuery = q.Encode()

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

					stat, err := file.Stat()
					if err != nil {
						return err
					}

					bar := progressbar.DefaultBytes(
						stat.Size(),
						"Upload "+file.Name(),
					)

					// io.Copy will return ErrClosedPipe when the server closes the connection or if the network connection is lost during file upload
					// Ignoring this error allows us to handle the error thrown from the request.Do() call
					// This will give us a more informative error in the case where the server closes the connection, because we can check the response status code
					// In the case of a network connection loss, request.Do() should still return an error, which we can handle
					if _, err := io.Copy(io.MultiWriter(formWriter, bar), file); err != nil && !errors.Is(err, io.ErrClosedPipe) {
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
				body, err := io.ReadAll(res.Body)
				if err != nil {
					return fmt.Errorf("error reading server response: %s", err.Error())
				}

				bodyReader := bytes.NewReader(body)

				var serverError types.ErrResponse
				if err := json.NewDecoder(bodyReader).Decode(&serverError); err != nil {
					log.Printf("body: %s", string(body))
					return fmt.Errorf("error decoding server response: %s", err.Error())
				}

				err = fmt.Errorf("API returned error: %s", serverError.ErrorText)

				if res.StatusCode == 404 || res.StatusCode == 401 || res.StatusCode == 400 {
					return backoff.Permanent(err)
				}

				return err
			}

			return nil
		}, backoff.NewExponentialBackOff(), func(err error, d time.Duration) {
			log.Printf("retrying in %d seconds, upload failed with: %s", int(d.Seconds()), err.Error())
		})
	},
}

func init() {
	rootCmd.AddCommand(uploadCmd)
	// Add to pkg subcommand as well, but we will still keep it in rootCmd
	// for backwards compatibility
	pkgCmd.AddCommand(uploadCmd)

	// Here you will define your flags and configuration settings.

	// Cobra supports Persistent Flags which will work for this command
	// and all subcommands, e.g.:
	// uploadCmd.PersistentFlags().String("foo", "", "A help for foo")

	// Cobra supports local flags which will only run when this command
	// is called directly, e.g.:
	// uploadCmd.Flags().BoolP("toggle", "t", false, "Help message for toggle")
	uploadCmd.Flags().Bool("prune", false, "Prune older packages on upload")
}
