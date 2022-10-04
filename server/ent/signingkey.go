// Code generated by ent, DO NOT EDIT.

package ent

import (
	"fmt"
	"strings"

	"entgo.io/ent/dialect/sql"
	"github.com/FyraLabs/subatomic/server/ent/signingkey"
)

// SigningKey is the model entity for the SigningKey schema.
type SigningKey struct {
	config `json:"-"`
	// ID of the ent.
	ID string `json:"id,omitempty"`
	// PrivateKey holds the value of the "private_key" field.
	PrivateKey string `json:"private_key,omitempty"`
	// PublicKey holds the value of the "public_key" field.
	PublicKey string `json:"public_key,omitempty"`
	// Name holds the value of the "name" field.
	Name string `json:"name,omitempty"`
	// Email holds the value of the "email" field.
	Email string `json:"email,omitempty"`
	// Edges holds the relations/edges for other nodes in the graph.
	// The values are being populated by the SigningKeyQuery when eager-loading is set.
	Edges SigningKeyEdges `json:"edges"`
}

// SigningKeyEdges holds the relations/edges for other nodes in the graph.
type SigningKeyEdges struct {
	// Repo holds the value of the repo edge.
	Repo []*Repo `json:"repo,omitempty"`
	// loadedTypes holds the information for reporting if a
	// type was loaded (or requested) in eager-loading or not.
	loadedTypes [1]bool
}

// RepoOrErr returns the Repo value or an error if the edge
// was not loaded in eager-loading.
func (e SigningKeyEdges) RepoOrErr() ([]*Repo, error) {
	if e.loadedTypes[0] {
		return e.Repo, nil
	}
	return nil, &NotLoadedError{edge: "repo"}
}

// scanValues returns the types for scanning values from sql.Rows.
func (*SigningKey) scanValues(columns []string) ([]interface{}, error) {
	values := make([]interface{}, len(columns))
	for i := range columns {
		switch columns[i] {
		case signingkey.FieldID, signingkey.FieldPrivateKey, signingkey.FieldPublicKey, signingkey.FieldName, signingkey.FieldEmail:
			values[i] = new(sql.NullString)
		default:
			return nil, fmt.Errorf("unexpected column %q for type SigningKey", columns[i])
		}
	}
	return values, nil
}

// assignValues assigns the values that were returned from sql.Rows (after scanning)
// to the SigningKey fields.
func (sk *SigningKey) assignValues(columns []string, values []interface{}) error {
	if m, n := len(values), len(columns); m < n {
		return fmt.Errorf("mismatch number of scan values: %d != %d", m, n)
	}
	for i := range columns {
		switch columns[i] {
		case signingkey.FieldID:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field id", values[i])
			} else if value.Valid {
				sk.ID = value.String
			}
		case signingkey.FieldPrivateKey:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field private_key", values[i])
			} else if value.Valid {
				sk.PrivateKey = value.String
			}
		case signingkey.FieldPublicKey:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field public_key", values[i])
			} else if value.Valid {
				sk.PublicKey = value.String
			}
		case signingkey.FieldName:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field name", values[i])
			} else if value.Valid {
				sk.Name = value.String
			}
		case signingkey.FieldEmail:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field email", values[i])
			} else if value.Valid {
				sk.Email = value.String
			}
		}
	}
	return nil
}

// QueryRepo queries the "repo" edge of the SigningKey entity.
func (sk *SigningKey) QueryRepo() *RepoQuery {
	return (&SigningKeyClient{config: sk.config}).QueryRepo(sk)
}

// Update returns a builder for updating this SigningKey.
// Note that you need to call SigningKey.Unwrap() before calling this method if this SigningKey
// was returned from a transaction, and the transaction was committed or rolled back.
func (sk *SigningKey) Update() *SigningKeyUpdateOne {
	return (&SigningKeyClient{config: sk.config}).UpdateOne(sk)
}

// Unwrap unwraps the SigningKey entity that was returned from a transaction after it was closed,
// so that all future queries will be executed through the driver which created the transaction.
func (sk *SigningKey) Unwrap() *SigningKey {
	_tx, ok := sk.config.driver.(*txDriver)
	if !ok {
		panic("ent: SigningKey is not a transactional entity")
	}
	sk.config.driver = _tx.drv
	return sk
}

// String implements the fmt.Stringer.
func (sk *SigningKey) String() string {
	var builder strings.Builder
	builder.WriteString("SigningKey(")
	builder.WriteString(fmt.Sprintf("id=%v, ", sk.ID))
	builder.WriteString("private_key=")
	builder.WriteString(sk.PrivateKey)
	builder.WriteString(", ")
	builder.WriteString("public_key=")
	builder.WriteString(sk.PublicKey)
	builder.WriteString(", ")
	builder.WriteString("name=")
	builder.WriteString(sk.Name)
	builder.WriteString(", ")
	builder.WriteString("email=")
	builder.WriteString(sk.Email)
	builder.WriteByte(')')
	return builder.String()
}

// SigningKeys is a parsable slice of SigningKey.
type SigningKeys []*SigningKey

func (sk SigningKeys) config(cfg config) {
	for _i := range sk {
		sk[_i].config = cfg
	}
}
