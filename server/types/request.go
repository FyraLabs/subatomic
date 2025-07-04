package types

import (
	"net/http"

	"github.com/go-playground/validator/v10"
)

type CreateRepoPayload struct {
	ID       string `json:"id" validate:"required,hostname"`
	RepoType string `json:"type" validate:"required,oneof='rpm'"`
}

func (u *CreateRepoPayload) Bind(r *http.Request) error {
	validate := r.Context().Value(ValidateContextKey{}).(*validator.Validate)
	return validate.Struct(u)
}

type BulkDeleteRPMsPayload struct {
	IDs []int `json:"ids" validate:"required,gt=0"`
}

func (u *BulkDeleteRPMsPayload) Bind(r *http.Request) error {
	validate := r.Context().Value(ValidateContextKey{}).(*validator.Validate)
	return validate.Struct(u)
}

type CreateKeyPayload struct {
	ID    string `json:"id" validate:"required,hostname"`
	Name  string `json:"name" validate:"required"`
	Email string `json:"email" validate:"required,email"`
}

func (u *CreateKeyPayload) Bind(r *http.Request) error {
	validate := r.Context().Value(ValidateContextKey{}).(*validator.Validate)
	return validate.Struct(u)
}

type SetKeyPayload struct {
	ID string `json:"id" validate:"required,hostname"`
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
	FilePath     *string `form:"file_path"`
}
