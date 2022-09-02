package main

import (
	"errors"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path"

	"github.com/FyraLabs/subatomic/ent"
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/render"
	"github.com/google/uuid"
	"github.com/ostreedev/ostree-go/pkg/otbuiltin"

	"github.com/FyraLabs/subatomic/ent/repo"
)

type reposRouter struct {
	*chi.Mux
	database   *ent.Client
	enviroment *Enviroment
}

func (router *reposRouter) setup() {
	router.Mux = chi.NewRouter()

	router.Get("/", router.getRepos)
	router.Post("/", router.createRepo)
	router.Delete("/{repoID}", router.deleteRepo)
	router.Put("/{repoID}", router.uploadToRepo)
}

// getRepos godoc
// @Summary     Get all repos
// @Description get repos
// @Tags        repos
// @Produce     json
// @Success     200 {array} ent.Repo
// @Router      /repos [get]
func (router *reposRouter) getRepos(w http.ResponseWriter, r *http.Request) {
	repos, err := router.database.Repo.Query().All(r.Context())

	if err != nil {
		panic(err)
	}

	render.JSON(w, r, repos)
}

type createRepoPayload struct {
	ID       string `json:"id" validate:"required,alphanum"`
	RepoType string `json:"type" validate:"required,oneof='dnf' 'ostree'"`
}

func (u *createRepoPayload) Bind(r *http.Request) error {
	return validate.Struct(u)
}

// createRepo godoc
// @Summary     Create a new repo
// @Description create repo
// @Tags        repos
// @Accept      json
// @Param       id body      string true "id for the new repository"
// @Param       id body repo.Type true "type for the new repository"
// @Success     200
// @Failure     400 {object} ErrResponse
// @Failure     409 {object} ErrResponse
// @Router      /repos [post]
func (router *reposRouter) createRepo(w http.ResponseWriter, r *http.Request) {
	payload := &createRepoPayload{}

	if err := render.Bind(r, payload); err != nil {
		render.Render(w, r, ErrInvalidRequest(err))
		return
	}

	exists, err := router.database.Repo.Query().Where(repo.IDEQ(payload.ID)).Exist(r.Context())

	if err != nil {
		panic(err)
	}

	if exists {
		render.Render(w, r, ErrAlreadyExists(errors.New("repo already exists")))
		return
	}

	repositoryDir := path.Join(router.enviroment.StorageDirectory, payload.ID)

	switch payload.RepoType {
	case "dnf":
		if err := os.MkdirAll(repositoryDir, os.ModePerm); err != nil {
			panic(err)
		}

		if _, err := exec.Command("createrepo_c", repositoryDir).Output(); err != nil {
			panic(err)
		}
	case "ostree":
		options := otbuiltin.NewInitOptions()
		options.Mode = "bare"
		if _, err := otbuiltin.Init(repositoryDir, options); err != nil {
			panic(err)
		}
	}

	_, err = router.database.Repo.Create().SetID(payload.ID).SetType(repo.Type(payload.RepoType)).Save(r.Context())

	if err != nil {
		panic(err)
	}

	w.WriteHeader(http.StatusCreated)
	if _, err := w.Write(nil); err != nil {
		panic(err)
	}
}

// deleteRepo godoc
// @Summary     Delete a repo
// @Description delete repo
// @Tags        repos
// @Param       id path string true "id for the repository"
// @Success     200
// @Failure     404 {object} ErrResponse
// @Router      /repos/{id} [delete]
func (router *reposRouter) deleteRepo(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")

	repo, err := router.database.Repo.Get(r.Context(), id)

	if ent.IsNotFound(err) {
		render.Render(w, r, ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	if err := os.RemoveAll(path.Join(router.enviroment.StorageDirectory, id)); err != nil {
		panic(err)
	}

	if err := router.database.Repo.DeleteOne(repo).Exec(r.Context()); err != nil {
		panic(err)
	}

	w.WriteHeader(http.StatusNoContent)
	if _, err := w.Write(nil); err != nil {
		panic(err)
	}
}

// uploadToRepo godoc
// @Summary     Upload files to a repo
// @Description upload to repo
// @Tags        repos
// @Param       id          path     string true "id for the repository"
// @Param       file_upload formData string true "files to upload to this reposiutory"
// @Accept      mpfd
// @Success     200
// @Failure     400 {object} ErrResponse
// @Failure     404 {object} ErrResponse
// @Router      /repos/{id} [put]
func (router *reposRouter) uploadToRepo(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")

	re, err := router.database.Repo.Get(r.Context(), id)

	if ent.IsNotFound(err) {
		render.Render(w, r, ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	switch re.Type {
	case repo.TypeDnf:
		if r.ParseMultipartForm(32 << 20); err != nil {
			panic(err)
		}

		files := r.MultipartForm.File["file_upload"]
		targetDirectory := path.Join(router.enviroment.StorageDirectory, id)

		for _, fileHeader := range files {
			reqFile, err := fileHeader.Open()
			if err != nil {
				panic(err)
			}

			defer reqFile.Close()

			file, err := os.Create(path.Join(targetDirectory, uuid.NewString()+".rpm"))

			if err != nil {
				panic(err)
			}

			defer file.Close()

			_, err = io.Copy(file, reqFile)
			if err != nil {
				render.Render(w, r, ErrInvalidRequest(err))
			}
		}

		// TODO: Remember to lock this
		// TODO: Also siging the repodata
		if _, err := exec.Command("createrepo_c", "--update", targetDirectory).Output(); err != nil {
			panic(err)
		}

		w.WriteHeader(http.StatusNoContent)

		if _, err := w.Write(nil); err != nil {
			panic(err)
		}
	case repo.TypeOstree:
		panic("not supported")
	}
}
