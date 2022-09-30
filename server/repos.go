package main

import (
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/FyraLabs/subatomic/server/types"
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/render"
	"github.com/ostreedev/ostree-go/pkg/otbuiltin"
	"github.com/samber/lo"

	"github.com/FyraLabs/subatomic/server/ent/predicate"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/rpmpackage"
	rpm "github.com/sassoftware/go-rpmutils"
)

type reposRouter struct {
	*chi.Mux
	database   *ent.Client
	enviroment *types.Enviroment
}

func (router *reposRouter) setup() {
	router.Mux = chi.NewRouter()

	router.Get("/", router.getRepos)
	router.Post("/", router.createRepo)
	router.Delete("/{repoID}", router.deleteRepo)
	router.Put("/{repoID}", router.uploadToRepo)

	// RPM Specific Endpoints
	router.Get("/{repoID}/rpms", router.getRPMs)
}

type repoResponse struct {
	ID   string `json:"id"`
	Type string `json:"type"`
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

	res := lo.Map(repos, func(repo *ent.Repo, _ int) *repoResponse {
		return &repoResponse{
			ID:   repo.ID,
			Type: string(repo.Type),
		}
	})

	render.JSON(w, r, res)
}

type createRepoPayload struct {
	ID       string `json:"id" validate:"required,alphanum"`
	RepoType string `json:"type" validate:"required,oneof='rpm' 'ostree'"`
}

func (u *createRepoPayload) Bind(r *http.Request) error {
	return validate.Struct(u)
}

// createRepo godoc
// @Summary     Create a new repo
// @Description create repo
// @Tags        repos
// @Accept      json
// @Param       body body createRepoPayload true "options for the new repository"
// @Success     200
// @Failure     400 {object} ErrResponse
// @Failure     409 {object} ErrResponse
// @Router      /repos [post]
func (router *reposRouter) createRepo(w http.ResponseWriter, r *http.Request) {
	payload := &createRepoPayload{}

	if err := render.Bind(r, payload); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	// TODO: Make atomic
	exists, err := router.database.Repo.Query().Where(repo.IDEQ(payload.ID)).Exist(r.Context())

	if err != nil {
		panic(err)
	}

	if exists {
		render.Render(w, r, types.ErrAlreadyExists(errors.New("repo already exists")))
		return
	}

	repositoryDir := path.Join(router.enviroment.StorageDirectory, payload.ID)

	switch payload.RepoType {
	case "rpm":
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
		render.Render(w, r, types.ErrNotFound(errors.New("repo not found")))
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
		render.Render(w, r, types.ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	switch re.Type {
	case repo.TypeRpm:
		if r.ParseMultipartForm(32 << 20); err != nil {
			panic(err)
		}

		if r.MultipartForm == nil {
			render.Render(w, r, types.ErrInvalidRequest(errors.New("request body must be multipart")))
			return
		}

		files, ok := r.MultipartForm.File["file_upload"]
		if !ok {
			render.Render(w, r, types.ErrInvalidRequest(errors.New("no files passed under key file_upload")))
			return
		}
		targetDirectory := path.Join(router.enviroment.StorageDirectory, id)

		for _, fileHeader := range files {
			reqFile, err := fileHeader.Open()
			if err != nil {
				panic(err)
			}

			defer reqFile.Close()

			rpmPackage, err := rpm.ReadRpm(reqFile)
			if err != nil {
				render.Render(w, r, types.ErrInvalidRequest(fmt.Errorf("rpm %s not valid", fileHeader.Filename)))
				return
			}

			nevra, err := rpmPackage.Header.GetNEVRA()
			if err != nil {
				render.Render(w, r, types.ErrInvalidRequest(fmt.Errorf("rpm %s nevra not valid", fileHeader.Filename)))
				return
			}

			nevraString := nevra.String()

			exists, err := re.QueryRpms().Where(
				rpmpackage.And(
					rpmpackage.NameEQ(nevra.Name),
					rpmpackage.EpochEQ(nevra.Epoch),
					rpmpackage.VersionEQ(nevra.Version),
					rpmpackage.ReleaseEQ(nevra.Release),
					rpmpackage.ArchEQ(nevra.Arch),
				)).Exist(r.Context())
			if err != nil {
				panic(err)
			}

			if exists {
				render.Render(w, r, types.ErrAlreadyExists(fmt.Errorf("rpm %s already exists", nevra)))
				return
			}

			_, err = router.database.RpmPackage.Create().
				SetName(nevra.Name).
				SetEpoch(nevra.Epoch).
				SetVersion(nevra.Version).
				SetRelease(nevra.Release).
				SetArch(nevra.Arch).
				SetRepo(re).
				SetFilePath(nevraString).
				Save(r.Context())
			if err != nil {
				panic(err)
			}

			file, err := os.Create(path.Join(targetDirectory, nevraString))

			if err != nil {
				panic(err)
			}

			defer file.Close()

			_, err = io.Copy(file, reqFile)
			if err != nil {
				render.Render(w, r, types.ErrInvalidRequest(err))
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

// TODO: maybe we could add support for other package types
type rpmResponse struct {
	ID       int    `json:"id"`
	Name     string `json:"name"`
	Epoch    string `json:"epoch"`
	Version  string `json:"version"`
	Release  string `json:"release"`
	Arch     string `json:"arch"`
	FilePath string `json:"file_path"`
}

type queryRpmParams struct {
	Name         *string `form:"name"`
	NameContains *string `form:"name_contains"`
	Epoch        *string `form:"epoch"`
	Version      *string `form:"version"`
	Release      *string `form:"release"`
	Arch         *string `form:"arch"`
}

// uploadToRepo godoc
// @Summary     Get list of RPMs in a repo
// @Description rpms in repo
// @Tags        repos
// @Param       id path string true "id for the repository"
// @Success     200
// @Failure     404 {object} ErrResponse
// @Router      /repos/{id}/rpms [get]
func (router *reposRouter) getRPMs(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")

	query := &queryRpmParams{}

	if err := decoder.Decode(query, r.URL.Query()); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	re, err := router.database.Repo.Get(r.Context(), id)

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	if re.Type != repo.TypeRpm {
		render.Render(w, r, types.ErrInvalidRequest(errors.New("can't query RPMs for a non-rpm repo")))
		return
	}

	predicates := []predicate.RpmPackage{}

	if query.Name != nil {
		predicates = append(predicates, rpmpackage.NameEQ(*query.Name))
	}

	if query.NameContains != nil {
		predicates = append(predicates, rpmpackage.NameContains(*query.NameContains))
	}

	if query.Epoch != nil {
		predicates = append(predicates, rpmpackage.EpochEQ(*query.Epoch))
	}

	if query.Version != nil {
		predicates = append(predicates, rpmpackage.VersionEQ(*query.Version))
	}

	if query.Release != nil {
		predicates = append(predicates, rpmpackage.ReleaseEQ(*query.Release))
	}

	if query.Arch != nil {
		predicates = append(predicates, rpmpackage.ArchEQ(*query.Arch))
	}

	var rpms []*ent.RpmPackage

	if len(predicates) == 0 {
		rpms, err = re.QueryRpms().All(r.Context())
	} else {
		rpms, err = re.QueryRpms().Where(rpmpackage.And(predicates...)).All(r.Context())
	}

	if err != nil {
		panic(err)
	}

	res := lo.Map(rpms, func(pkg *ent.RpmPackage, _ int) *rpmResponse {
		return &rpmResponse{
			ID:       pkg.ID,
			Name:     pkg.Name,
			Epoch:    pkg.Epoch,
			Version:  pkg.Version,
			Release:  pkg.Release,
			Arch:     pkg.Arch,
			FilePath: pkg.FilePath,
		}
	})

	render.JSON(w, r, res)
}
