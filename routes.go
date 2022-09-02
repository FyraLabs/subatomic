package main

import (
	"net/http"

	"github.com/FyraLabs/subatomic/ent"
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
)

type mainRouter struct {
	*chi.Mux
	database   *ent.Client
	enviroment *Enviroment
}

func (router *mainRouter) setup() {
	router.Mux = chi.NewRouter()

	router.Use(middleware.Logger)
	router.Use(middleware.Recoverer)
	router.Use(middleware.Heartbeat("/api/heartbeat"))

	api := apiRouter{
		database:   router.database,
		enviroment: router.enviroment,
	}
	api.setup()
	router.Mount("/api", api)

	// TODO: Can we make this more pretty?
	fileServer := http.FileServer(http.Dir(router.enviroment.StorageDirectory))
	router.Handle("/*", fileServer)
}
