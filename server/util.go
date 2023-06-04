package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"runtime/debug"

	"github.com/FyraLabs/subatomic/server/types"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/go-chi/render"

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

func recovererMiddleware(next http.Handler) http.Handler {
	fn := func(w http.ResponseWriter, r *http.Request) {
		defer func() {
			if rvr := recover(); rvr != nil && rvr != http.ErrAbortHandler {
				logEntry := middleware.GetLogEntry(r)
				if logEntry != nil {
					logEntry.Panic(rvr, debug.Stack())
				} else {
					middleware.PrintPrettyStack(rvr)
				}

				var err error

				if e, ok := rvr.(error); ok {
					err = e
				} else {
					err = fmt.Errorf("unknown error")
				}

				render.Render(w, r, types.ErrInternalServerError(err))
			}
		}()

		next.ServeHTTP(w, r)
	}

	return http.HandlerFunc(fn)
}
