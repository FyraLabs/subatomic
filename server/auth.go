package main

import (
	"context"
	"errors"
	"net/http"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/go-chi/jwtauth/v5"
	"github.com/go-chi/render"
	"github.com/lestrrat-go/jwx/v2/jwt"
	"golang.org/x/exp/slices"
)

func ToSliceOf[T any](input []any) ([]T, bool) {
	result := make([]T, len(input))
	for i, v := range input {
		value, ok := v.(T)
		if !ok {
			return nil, false
		}
		result[i] = value
	}

	return result, true
}

func HasScopes(requiredScopes []string) jwt.ValidatorFunc {
	return jwt.ValidatorFunc(func(_ context.Context, t jwt.Token) jwt.ValidationError {
		scopesField, ok := t.Get("scopes")
		if !ok {
			return jwt.NewValidationError(errors.New("scopes field must be set"))
		}

		scopesArr, ok := scopesField.([]any)
		if !ok {
			return jwt.NewValidationError(errors.New("scopes field must be an array"))
		}

		scopes, ok := ToSliceOf[string](scopesArr)
		if !ok {
			return jwt.NewValidationError(errors.New("members in the scopes array must be strings"))
		}

		for _, scope := range requiredScopes {
			if !slices.Contains(scopes, scope) {
				return jwt.NewValidationError(errors.New("the scope \"" + scope + "\" is required"))
			}
		}

		return nil
	})
}

func Authenticator(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		token, _, err := jwtauth.FromContext(r.Context())

		if err != nil {
			render.Render(w, r, types.ErrUnauthorized(err))
			return
		}

		if token == nil {
			render.Render(w, r, types.ErrUnauthorized(errors.New("no token found")))
			return
		}

		// TODO: For now, we require that all tokens have the admin scope
		if err := jwt.Validate(token, jwt.WithValidator(HasScopes([]string{"admin"}))); err != nil {
			render.Render(w, r, types.ErrUnauthorized(err))
			return
		}

		// Token is authenticated, pass it through
		next.ServeHTTP(w, r)
	})
}
