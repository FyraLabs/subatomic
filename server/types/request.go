package types

import (
	"net/http"

	"github.com/go-playground/validator/v10"
)

type CreateRepoPayload struct {
	ID       string `json:"id" validate:"required,alphanum"`
	RepoType string `json:"type" validate:"required,oneof='rpm' 'ostree'"`
}

func (u *CreateRepoPayload) Bind(r *http.Request) error {
	validate := r.Context().Value(ValidateContextKey{}).(*validator.Validate)
	return validate.Struct(u)
}

type SetKeyPayload struct {
	ID string `json:"id" validate:"required,alphanum"`
}

func (u *SetKeyPayload) Bind(r *http.Request) error {
	validate := r.Context().Value(ValidateContextKey{}).(*validator.Validate)
	return validate.Struct(u)
}

type QueryRpmParams struct {
	Name         *string `form:"name"`
	NameContains *string `form:"name_contains"`
	Epoch        *int    `form:"epoch"`
	Version      *string `form:"version"`
	Release      *string `form:"release"`
	Arch         *string `form:"arch"`
	FilePath     *string `json:"file_path"`
}
