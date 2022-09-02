package main

import (
	"net/http"

	"github.com/FyraLabs/subatomic/ent"
	"github.com/go-chi/chi/v5"
	httpSwagger "github.com/swaggo/http-swagger"
)

type apiRouter struct {
	*chi.Mux
	database   *ent.Client
	enviroment *Enviroment
}

func (router *apiRouter) setup() {
	router.Mux = chi.NewRouter()

	repos := reposRouter{
		database:   router.database,
		enviroment: router.enviroment,
	}
	repos.setup()

	router.Mount("/repos", repos)
	router.Handle("/docs", http.RedirectHandler("/api/docs/index.html", http.StatusFound))
	router.Get("/docs/*", httpSwagger.Handler(
		httpSwagger.URL("/api/docs/doc.json"), //The url pointing to API definition
	))
}
