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
)

type keysRouter struct {
	*chi.Mux
	database    *ent.Client
	environment *types.Environment
}

func (router *keysRouter) setup() {
	router.Mux = chi.NewRouter()

	router.Get("/", router.getKeys)
	router.Post("/", router.createKey)

	router.Get("/{keyID}", router.getKey)
}

// getKeys godoc
//
//	@Summary		Get all keys
//	@Description	get keys
//	@Tags			keys
//	@Produce		json
//	@Success		200	{array}	types.KeyResponse
//	@Router			/keys [get]
func (router *keysRouter) getKeys(w http.ResponseWriter, r *http.Request) {
	keys, err := router.database.SigningKey.Query().All(r.Context())

	if err != nil {
		panic(err)
	}

	res := lo.Map(keys, func(key *ent.SigningKey, _ int) *types.KeyResponse {
		return &types.KeyResponse{
			ID:    key.ID,
			Name:  key.Name,
			Email: key.Email,
		}
	})

	render.JSON(w, r, res)
}

// createKey godoc
//
//	@Summary		Create a new key
//	@Description	create key
//	@Tags			keys
//	@Accept			json
//	@Param			body	body	types.CreateKeyPayload	true	"options for the new key"
//	@Success		201
//	@Failure		400	{object}	types.ErrResponse
//	@Failure		409	{object}	types.ErrResponse
//	@Router			/keys [post]
func (router *keysRouter) createKey(w http.ResponseWriter, r *http.Request) {
	payload := &types.CreateKeyPayload{}

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

	_, err = router.database.SigningKey.Create().
		SetID(payload.ID).
		SetPublicKey(armoredPublicKey).
		SetPrivateKey(armoredPrivateKey).
		SetEmail(payload.Email).
		SetName(payload.Name).
		Save(r.Context())

	if err != nil {
		panic(err)
	}

	w.WriteHeader(http.StatusCreated)
	if _, err := w.Write(nil); err != nil {
		panic(err)
	}
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
	if err := validate.Var(keyID, "required,hostname"); err != nil {
		render.Render(w, r, types.ErrInvalidRequest(err))
		return
	}

	key, err := router.database.SigningKey.Get(r.Context(), keyID)

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
