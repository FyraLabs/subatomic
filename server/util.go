package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"runtime/debug"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/go-chi/render"
	kitlog "github.com/go-kit/log"
	"github.com/go-kit/log/level"

	"go.opentelemetry.io/otel/exporters/otlp/otlptrace"
	"go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracehttp"
	"go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	semconv "go.opentelemetry.io/otel/semconv/v1.4.0"
)

func initTracerProvider() *sdktrace.TracerProvider {
	client := otlptracehttp.NewClient()
	exporter, err := otlptrace.New(context.Background(), client)
	if err != nil {
		log.Fatal(err)
	}

	res, err := resource.New(
		context.Background(),
		resource.WithAttributes(
			semconv.ServiceNameKey.String("subatomic"),
		),
	)
	if err != nil {
		log.Fatalf("unable to initialize resource due: %v", err)
	}

	return sdktrace.NewTracerProvider(
		sdktrace.WithSampler(sdktrace.AlwaysSample()),
		sdktrace.WithBatcher(exporter),
		sdktrace.WithResource(res),
	)
}
func recovererMiddleware(l kitlog.Logger) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		fn := func(w http.ResponseWriter, r *http.Request) {
			defer func() {
				if rvr := recover(); rvr != nil && rvr != http.ErrAbortHandler {
					// Log the recovered panic and the stack trace using the logger.
					// Convert the panic value and stack trace to appropriate types for logging.
					level.Error(l).Log(
						"msg", "recovered panic in HTTP handler",
						"panic", fmt.Sprintf("%v", rvr),
						"stack", string(debug.Stack()),
						"method", r.Method,
						"url", r.URL.String(),
						"remote", r.RemoteAddr,
					)
					var err error

					if e, ok := rvr.(error); ok {
						err = e
					} else {
						// For unknown panics, create an error that includes the panic value
						err = fmt.Errorf("unknown error: %v", rvr)
					}

					render.Render(w, r, types.ErrInternalServerError(err))
				}
			}()

			next.ServeHTTP(w, r)
		}

		return http.HandlerFunc(fn)
	}
}
