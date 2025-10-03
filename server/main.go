package main

import (
	"context"
	"fmt"
	"net/http"

	_ "github.com/FyraLabs/subatomic/server/docs"
	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/FyraLabs/subatomic/server/keyedmutex"
	"github.com/FyraLabs/subatomic/server/logging"
	"github.com/FyraLabs/subatomic/server/types"
	"github.com/Netflix/go-env"
	"github.com/go-chi/jwtauth/v5"
	"github.com/go-kit/log"
	"github.com/go-kit/log/level"
	"github.com/go-playground/form/v4"
	"github.com/go-playground/validator/v10"
	"github.com/joho/godotenv"
	_ "github.com/lib/pq"
	_ "github.com/swaggo/files"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/propagation"
)

var validate *validator.Validate
var decoder *form.Decoder
var main_logger log.Logger = log.With(logging.Logger)

//	@title			Subatomic
//	@version		1.0
//	@description	A modern package delivery server.
//	@BasePath		/

//	@license.name	GPL3
//	@license.url	https://choosealicense.com/licenses/gpl-3.0/

// @securityDefinitions.apikey
// @in		header
// @name	Authorization
func main() {
	if err := run(main_logger); err != nil {
		level.Error(main_logger).Log("msg", "fatal error", "error", err)
	}
}

func run(logger log.Logger) error {
	validate = validator.New()
	decoder = form.NewDecoder()
	var environment types.Environment

	_ = godotenv.Load()
	_, err := env.UnmarshalFromEnviron(&environment)
	if err != nil {
		return err
	}

	client, err := ent.Open("postgres", environment.DatabaseOptions)
	if err != nil {
		return err
	}

	if err := client.Schema.Create(context.Background()); err != nil {
		return fmt.Errorf("failed creating schema resources: %w", err)
	}

	if environment.EnableTracing {
		tp := initTracerProvider()
		defer func() {
			if err := tp.Shutdown(context.Background()); err != nil {
				level.Error(logger).Log("msg", "error shutting down tracer provider", "error", err)
			}
		}()

		otel.SetTracerProvider(tp)
		otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator(propagation.TraceContext{}, propagation.Baggage{}))
	}

	// TODO: Auth
	router := &apiRouter{
		database:         client,
		environment:      &environment,
		jwtAuthenticator: jwtauth.New("HS256", []byte(environment.JWTSecret), nil),
		repoMutex:        &keyedmutex.KeyedMutex{},
		logger:           logger,
	}
	router.setup()

	level.Info(logger).Log("msg", "listening on port", "port", ":3000")
	return http.ListenAndServe(":3000", router)
}
