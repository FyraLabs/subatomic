package main

import (
	"errors"
	"fmt"
	"net/http"
	"os"
	"path"
	"path/filepath"
	"strconv"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/FyraLabs/subatomic/server/keyedmutex"
	"github.com/FyraLabs/subatomic/server/ostree"
	"github.com/FyraLabs/subatomic/server/rpm"
	"github.com/FyraLabs/subatomic/server/types"
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/render"
	"github.com/samber/lo"

	"github.com/FyraLabs/subatomic/server/ent/predicate"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/rpmpackage"

	pgp "github.com/ProtonMail/gopenpgp/v2/crypto"
)

type reposRouter struct {
	*chi.Mux
	database   *ent.Client
	enviroment *types.Enviroment
	repoMutex  *keyedmutex.KeyedMutex
}

func (router *reposRouter) setup() {
	router.Mux = chi.NewRouter()

	router.Get("/", router.getRepos)
	router.Post("/", router.createRepo)
	router.Delete("/{repoID}", router.deleteRepo)
	router.Put("/{repoID}", router.uploadToRepo)

	// Repo Key
	router.Get("/{repoID}/key", router.getRepoKey)
	router.Put("/{repoID}/key", router.setRepoKey)
	router.Delete("/{repoID}/key", router.deleteRepoKey)

	// Signature Management
	router.Post("/{repoID}/resign", router.resign)

	// RPM Specific Endpoints
	router.Get("/{repoID}/rpms", router.getRPMs)
	router.Delete("/{repoID}/rpms/{rpmID}", router.deleteRPM)
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
// @Success     200 {array} repoResponse
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
// @Failure     400 {object} types.ErrResponse
// @Failure     409 {object} types.ErrResponse
// @Router      /repos [post]
func (router *reposRouter) createRepo(w http.ResponseWriter, r *http.Request) {
	payload := &createRepoPayload{}

	if err := render.Bind(r, payload); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	router.repoMutex.Lock(payload.ID)
	defer router.repoMutex.Unlock(payload.ID)

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
		if err := rpm.CreateRepo(repositoryDir); err != nil {
			panic(err)
		}
	case "ostree":
		if err := ostree.CreateRepo(repositoryDir); err != nil {
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
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id} [delete]
func (router *reposRouter) deleteRepo(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	router.repoMutex.Lock(id)
	defer router.repoMutex.Unlock(id)

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
// @Failure     400 {object} types.ErrResponse
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id} [put]
func (router *reposRouter) uploadToRepo(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	router.repoMutex.Lock(id)
	defer router.repoMutex.Unlock(id)

	re, err := router.database.Repo.Get(r.Context(), id)

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	key, err := re.QueryKey().Only(r.Context())
	if err != nil && !ent.IsNotFound(err) {
		panic(err)
	}

	var ring *pgp.KeyRing

	if key != nil {
		privateKey, err := pgp.NewKeyFromArmored(key.PrivateKey)
		if err != nil {
			panic(err)
		}

		ring, err = pgp.NewKeyRing(privateKey)
		if err != nil {
			panic(err)
		}
	}

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

	switch re.Type {
	case repo.TypeRpm:
		for _, fileHeader := range files {
			reqFile, err := fileHeader.Open()
			if err != nil {
				panic(err)
			}

			defer reqFile.Close()

			info, err := rpm.GetRpmInfo(reqFile)
			if err != nil {
				render.Render(w, r, types.ErrInvalidRequest(err))
				return
			}

			exists, err := re.QueryRpms().Where(
				rpmpackage.And(
					rpmpackage.NameEQ(info.Name),
					rpmpackage.EpochEQ(info.Epoch),
					rpmpackage.VersionEQ(info.Version),
					rpmpackage.ReleaseEQ(info.Release),
					rpmpackage.ArchEQ(info.Arch),
				)).Exist(r.Context())
			if err != nil {
				panic(err)
			}

			if exists {
				render.Render(w, r, types.ErrAlreadyExists(fmt.Errorf("rpm %s already exists", info.FileName)))
				return
			}

			_, err = router.database.RpmPackage.Create().
				SetName(info.Name).
				SetEpoch(info.Epoch).
				SetVersion(info.Version).
				SetRelease(info.Release).
				SetArch(info.Arch).
				SetRepo(re).
				SetFilePath(info.FileName).
				Save(r.Context())
			if err != nil {
				panic(err)
			}

			if err := rpm.AddRpmToRepo(targetDirectory, reqFile); err != nil {
				panic(err)
			}

			if ring != nil {
				rpmPath := path.Join(targetDirectory, info.FileName)
				if err := rpm.SignRpmFile(rpmPath, ring); err != nil {
					panic(err)
				}
			}
		}

		// TODO: Also siging the repodata
		if err := rpm.UpdateRepo(targetDirectory); err != nil {
			panic(err)
		}

		if ring != nil {
			if err := rpm.SignRepo(targetDirectory, ring); err != nil {
				panic(err)
			}
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
	Epoch    int    `json:"epoch"`
	Version  string `json:"version"`
	Release  string `json:"release"`
	Arch     string `json:"arch"`
	FilePath string `json:"file_path"`
}

type queryRpmParams struct {
	Name         *string `form:"name"`
	NameContains *string `form:"name_contains"`
	Epoch        *int    `form:"epoch"`
	Version      *string `form:"version"`
	Release      *string `form:"release"`
	Arch         *string `form:"arch"`
	FilePath     *string `json:"file_path"`
}

// getRPMs godoc
// @Summary     Get list of RPMs in a repo
// @Description rpms in repo
// @Tags        repos
// @Param       id path string true "id for the repository"
// @Success     200
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id}/rpms [get]
func (router *reposRouter) getRPMs(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

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

	if query.FilePath != nil {
		predicates = append(predicates, rpmpackage.FilePathEQ(*query.FilePath))
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

// deleteRPM godoc
// @Summary     Delete RPM in a repo
// @Description delete rpm
// @Tags        repos
// @Param       id    path string true "id for the repository"
// @Param       rpmID path string true "rpm id in the repository"
// @Success     200
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id}/rpms/{rpmID} [delete]
func (router *reposRouter) deleteRPM(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	router.repoMutex.Lock(id)
	defer router.repoMutex.Unlock(id)

	rpmID, err := strconv.Atoi(chi.URLParam(r, "rpmID"))
	if err != nil {
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

	rpm, err := re.QueryRpms().Where(rpmpackage.IDEQ(rpmID)).First(r.Context())

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("rpm not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	if err := router.database.RpmPackage.DeleteOne(rpm).Exec(r.Context()); err != nil {
		panic(err)
	}

	targetDirectory := path.Join(router.enviroment.StorageDirectory, id, rpm.FilePath)

	if err := os.Remove(targetDirectory); err != nil {
		panic(err)
	}

	w.WriteHeader(http.StatusNoContent)

	if _, err := w.Write(nil); err != nil {
		panic(err)
	}
}

// getRepoKey godoc
// @Summary     Get key for a repo
// @Description get repo key
// @Tags        repos
// @Param       id path string true "id for the repository"
// @Produce     json
// @Success     200 {object} fullKeyResponse
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id}/key [get]
func (router *reposRouter) getRepoKey(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
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

	key, err := re.QueryKey().First(r.Context())

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("key not set")))
		return
	}

	if err != nil {
		panic(err)
	}

	render.JSON(w, r, &fullKeyResponse{
		ID:        key.ID,
		Name:      key.Name,
		Email:     key.Email,
		PublicKey: key.PublicKey,
	})
}

type setKeyPayload struct {
	ID string `json:"id" validate:"required,alphanum"`
}

func (u *setKeyPayload) Bind(r *http.Request) error {
	return validate.Struct(u)
}

// setRepoKey godoc
// @Summary     Set key for a repo
// @Description set repo key
// @Tags        repos
// @Param       id   path string            true "id for the repository"
// @Param       body body createRepoPayload true "options for the new repository"
// @Produce     json
// @Success     204
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id}/key [put]
func (router *reposRouter) setRepoKey(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	router.repoMutex.Lock(id)
	defer router.repoMutex.Unlock(id)

	re, err := router.database.Repo.Get(r.Context(), id)

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	payload := &setKeyPayload{}
	if err := render.Bind(r, payload); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	key, err := router.database.SigningKey.Get(r.Context(), payload.ID)

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("key not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	if _, err := re.Update().SetKey(key).Save(r.Context()); err != nil {
		panic(err)
	}

	if err := os.WriteFile(path.Join(router.enviroment.StorageDirectory, id, "key.asc"), []byte(key.PublicKey), 0644); err != nil {
		panic(err)
	}

	w.WriteHeader(http.StatusNoContent)

	if _, err := w.Write(nil); err != nil {
		panic(err)
	}
}

// deleteRepoKey godoc
// @Summary     Delete key for a repo
// @Description delete repo key
// @Tags        repos
// @Param       id path string true "id for the repository"
// @Produce     json
// @Success     204
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id}/key [delete]
func (router *reposRouter) deleteRepoKey(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	router.repoMutex.Lock(id)
	defer router.repoMutex.Unlock(id)

	re, err := router.database.Repo.Get(r.Context(), id)

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	if _, err := re.Update().ClearKey().Save(r.Context()); err != nil {
		panic(err)
	}

	if err := os.Remove(path.Join(router.enviroment.StorageDirectory, id, "key.asc")); err != nil {
		panic(err)
	}

	w.WriteHeader(http.StatusNoContent)

	if _, err := w.Write(nil); err != nil {
		panic(err)
	}
}

// resign godoc
// @Summary     Resign packages in a repo
// @Description resign repo packages
// @Tags        repos
// @Param       id path string true "id for the repository"
// @Produce     json
// @Success     204
// @Failure     404 {object} types.ErrResponse
// @Router      /repos/{id}/resign [post]
func (router *reposRouter) resign(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "repoID")
	if err := validate.Var(id, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	router.repoMutex.Lock(id)
	defer router.repoMutex.Unlock(id)

	re, err := router.database.Repo.Get(r.Context(), id)

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("repo not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	key, err := re.QueryKey().Only(r.Context())
	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("key not set")))
		return
	}

	if err != nil {
		panic(err)
	}

	privateKey, err := pgp.NewKeyFromArmored(key.PrivateKey)
	if err != nil {
		panic(err)
	}

	ring, err := pgp.NewKeyRing(privateKey)
	if err != nil {
		panic(err)
	}

	targetDirectory := path.Join(router.enviroment.StorageDirectory, id)

	switch re.Type {
	case repo.TypeRpm:
		{
			matches, err := filepath.Glob(path.Join(targetDirectory, "*.rpm"))
			if err != nil {
				panic(err)
			}

			for _, match := range matches {
				if err := rpm.SignRpmFile(match, ring); err != nil {
					panic(err)
				}
			}

			if err := rpm.UpdateRepo(targetDirectory); err != nil {
				panic(err)
			}

			if err := rpm.SignRepo(targetDirectory, ring); err != nil {
				panic(err)
			}
		}
	case repo.TypeOstree:
		{
			panic("not supported")
		}
	}

	w.WriteHeader(http.StatusNoContent)

	if _, err := w.Write(nil); err != nil {
		panic(err)
	}
}
