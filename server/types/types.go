package types

import (
	"net/http"

	"github.com/go-chi/render"
)

type ErrResponse struct {
	Err            error `json:"-"` // low-level runtime error
	HTTPStatusCode int   `json:"-"` // http response status code

	StatusText string `json:"status"`          // user-level status message
	AppCode    int64  `json:"code,omitempty"`  // application-specific error code
	ErrorText  string `json:"error,omitempty"` // application-level error message, for debugging
}

func (e *ErrResponse) Render(w http.ResponseWriter, r *http.Request) error {
	render.Status(r, e.HTTPStatusCode)
	return nil
}

func ErrInvalidRequest(err error) render.Renderer {
	return &ErrResponse{
		Err:            err,
		HTTPStatusCode: 400,
		StatusText:     "Invalid request.",
		ErrorText:      err.Error(),
	}
}

func ErrNotFound(err error) render.Renderer {
	return &ErrResponse{
		Err:            err,
		HTTPStatusCode: 404,
		StatusText:     "Not found.",
		ErrorText:      err.Error(),
	}
}

func MethodNotAllowed(err error) render.Renderer {
	return &ErrResponse{
		Err:            err,
		HTTPStatusCode: 405,
		StatusText:     "Method not allowed.",
		ErrorText:      err.Error(),
	}
}

func ErrAlreadyExists(err error) render.Renderer {
	return &ErrResponse{
		Err:            err,
		HTTPStatusCode: 409,
		StatusText:     "Already exists.",
		ErrorText:      err.Error(),
	}
}

func ErrUnauthorized(err error) render.Renderer {
	return &ErrResponse{
		Err:            err,
		HTTPStatusCode: 401,
		StatusText:     "Unauthorized.",
		ErrorText:      err.Error(),
	}
}

type Enviroment struct {
	StorageDirectory string `env:"STORAGE_DIRECTORY,required=true"`
	DatabaseOptions  string `env:"DATABASE_OPTIONS,required=true"`
	JWTSecret        string `env:"JWT_SECRET,required=true"`
}
