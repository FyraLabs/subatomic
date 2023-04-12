package main

import (
	"errors"
	"net/http"

	"github.com/FyraLabs/subatomic/server/ent"
	"github.com/FyraLabs/subatomic/server/types"
	pgp "github.com/ProtonMail/gopenpgp/v2/crypto"
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/render"
	"github.com/samber/lo"
	"go.opentelemetry.io/otel/attribute"
	oteltrace "go.opentelemetry.io/otel/trace"
)

type keysRouter struct {
	*chi.Mux
	database   *ent.Client
	enviroment *types.Enviroment
	tracer     oteltrace.Tracer
}

func (router *keysRouter) setup() {
	router.Mux = chi.NewRouter()

	router.Get("/", router.getKeys)
	router.Post("/", router.createKey)

	router.Get("/{keyID}", router.getKey)
}

type keyResponse struct {
	ID    string `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
}

// getKeys godoc
//
//	@Summary		Get all keys
//	@Description	get keys
//	@Tags			keys
//	@Produce		json
//	@Success		200	{array}	keyResponse
//	@Router			/keys [get]
func (router *keysRouter) getKeys(w http.ResponseWriter, r *http.Request) {
	ctx, span := router.tracer.Start(r.Context(), "createKey")
	defer span.End()

	keys, err := router.database.SigningKey.Query().All(ctx)

	if err != nil {
		panic(err)
	}

	res := lo.Map(keys, func(key *ent.SigningKey, _ int) *keyResponse {
		return &keyResponse{
			ID:    key.ID,
			Name:  key.Name,
			Email: key.Email,
		}
	})

	render.JSON(w, r, res)
}

type createKeyPayload struct {
	ID    string `json:"id" validate:"required,alphanum"`
	Name  string `json:"name" validate:"required"`
	Email string `json:"email" validate:"required,email"`
}

func (u *createKeyPayload) Bind(r *http.Request) error {
	return validate.Struct(u)
}

// createKey godoc
//
//	@Summary		Create a new key
//	@Description	create key
//	@Tags			keys
//	@Accept			json
//	@Param			body	body	createKeyPayload	true	"options for the new key"
//	@Success		200
//	@Failure		400	{object}	types.ErrResponse
//	@Failure		409	{object}	types.ErrResponse
//	@Router			/keys [post]
func (router *keysRouter) createKey(w http.ResponseWriter, r *http.Request) {
	ctx, span := router.tracer.Start(r.Context(), "createKey")
	defer span.End()

	payload := &createKeyPayload{}

	if err := render.Bind(r, payload); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	privateKey, err := pgp.GenerateKey(payload.Name, payload.Email, "x25519", 0)
	if err != nil {
		panic(err)
	}

	armoredPrivateKey, err := privateKey.Armor()
	if err != nil {
		panic(err)
	}
	armoredPublicKey, err := privateKey.GetArmoredPublicKey()
	if err != nil {
		panic(err)
	}

	key, err := router.database.SigningKey.Create().
		SetID(payload.ID).
		SetPublicKey(armoredPublicKey).
		SetPrivateKey(armoredPrivateKey).
		SetEmail(payload.Email).
		SetName(payload.Name).
		Save(ctx)

	if err != nil {
		panic(err)
	}

	render.JSON(w, r, &keyResponse{
		ID:    key.ID,
		Name:  key.Name,
		Email: key.Email,
	})
}

type fullKeyResponse struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	Email     string `json:"email"`
	PublicKey string `json:"public_key"`
}

// getKey godoc
//
//	@Summary		Get a key
//	@Description	get key
//	@Tags			keys
//	@Param			id	path	string	true	"id for the key"
//	@Accept			json
//	@Success		200
//	@Failure		400	{object}	types.ErrResponse
//	@Failure		409	{object}	types.ErrResponse
//	@Router			/keys/{id} [get]
func (router *keysRouter) getKey(w http.ResponseWriter, r *http.Request) {
	keyID := chi.URLParam(r, "keyID")
	if err := validate.Var(keyID, "required,alphanum"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	ctx, span := router.tracer.Start(r.Context(), "getKey", oteltrace.WithAttributes(attribute.String("keyID", keyID)))
	defer span.End()

	key, err := router.database.SigningKey.Get(ctx, keyID)

	if ent.IsNotFound(err) {
		render.Render(w, r, types.ErrNotFound(errors.New("key not found")))
		return
	}

	if err != nil {
		panic(err)
	}

	render.JSON(w, r, &fullKeyResponse{
		ID:        key.ID,
		Name:      key.Name,
		Email:     key.Email,
		PublicKey: key.PublicKey,
	})
}
