package main

import (
	"net/http"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/FyraLabs/subatomic/server/keyedmutex"
	"github.com/FyraLabs/subatomic/server/types"
	"github.com/go-chi/chi/v5"
	httpSwagger "github.com/swaggo/http-swagger"

	"github.com/go-chi/jwtauth/v5"
)

type apiRouter struct {
	*chi.Mux
	database         *ent.Client
	enviroment       *types.Enviroment
	jwtAuthenticator *jwtauth.JWTAuth
	repoMutex        *keyedmutex.KeyedMutex
}

func (router *apiRouter) setup() {
	router.Mux = chi.NewRouter()

	// Authenticated
	router.Group(func(r chi.Router) {
		r.Use(jwtauth.Verifier(router.jwtAuthenticator))
		r.Use(Authenticator)

		repos := reposRouter{
			database:   router.database,
			enviroment: router.enviroment,
		}
		repos.setup()
		r.Mount("/repos", repos)

		keys := keysRouter{
			database:   router.database,
			enviroment: router.enviroment,
		}
		keys.setup()
		r.Mount("/keys", keys)
	})

	// Public
	router.Group(func(r chi.Router) {
		router.Handle("/docs", http.RedirectHandler("/api/docs/index.html", http.StatusFound))
		router.Get("/docs/*", httpSwagger.Handler(
			httpSwagger.URL("/api/docs/doc.json"), //The url pointing to API definition
		))
	})
}
