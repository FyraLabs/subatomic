// Code generated by ent, DO NOT EDIT.

package ent

import (
	"fmt"
	"strings"

	"entgo.io/ent"
	"entgo.io/ent/dialect/sql"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/signingkey"
)

// Repo is the model entity for the Repo schema.
type Repo struct {
	config `json:"-"`
	// ID of the ent.
	ID string `json:"id,omitempty"`
	// Type holds the value of the "type" field.
	Type repo.Type `json:"type,omitempty"`
	// Edges holds the relations/edges for other nodes in the graph.
	// The values are being populated by the RepoQuery when eager-loading is set.
	Edges        RepoEdges `json:"edges"`
	repo_key     *string
	selectValues sql.SelectValues
}

// RepoEdges holds the relations/edges for other nodes in the graph.
type RepoEdges struct {
	// Rpms holds the value of the rpms edge.
	Rpms []*RpmPackage `json:"rpms,omitempty"`
	// Key holds the value of the key edge.
	Key *SigningKey `json:"key,omitempty"`
	// loadedTypes holds the information for reporting if a
	// type was loaded (or requested) in eager-loading or not.
	loadedTypes [2]bool
}

// RpmsOrErr returns the Rpms value or an error if the edge
// was not loaded in eager-loading.
func (e RepoEdges) RpmsOrErr() ([]*RpmPackage, error) {
	if e.loadedTypes[0] {
		return e.Rpms, nil
	}
	return nil, &NotLoadedError{edge: "rpms"}
}

// KeyOrErr returns the Key value or an error if the edge
// was not loaded in eager-loading, or loaded but was not found.
func (e RepoEdges) KeyOrErr() (*SigningKey, error) {
	if e.Key != nil {
		return e.Key, nil
	} else if e.loadedTypes[1] {
		return nil, &NotFoundError{label: signingkey.Label}
	}
	return nil, &NotLoadedError{edge: "key"}
}

// scanValues returns the types for scanning values from sql.Rows.
func (*Repo) scanValues(columns []string) ([]any, error) {
	values := make([]any, len(columns))
	for i := range columns {
		switch columns[i] {
		case repo.FieldID, repo.FieldType:
			values[i] = new(sql.NullString)
		case repo.ForeignKeys[0]: // repo_key
			values[i] = new(sql.NullString)
		default:
			values[i] = new(sql.UnknownType)
		}
	}
	return values, nil
}

// assignValues assigns the values that were returned from sql.Rows (after scanning)
// to the Repo fields.
func (r *Repo) assignValues(columns []string, values []any) error {
	if m, n := len(values), len(columns); m < n {
		return fmt.Errorf("mismatch number of scan values: %d != %d", m, n)
	}
	for i := range columns {
		switch columns[i] {
		case repo.FieldID:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field id", values[i])
			} else if value.Valid {
				r.ID = value.String
			}
		case repo.FieldType:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field type", values[i])
			} else if value.Valid {
				r.Type = repo.Type(value.String)
			}
		case repo.ForeignKeys[0]:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field repo_key", values[i])
			} else if value.Valid {
				r.repo_key = new(string)
				*r.repo_key = value.String
			}
		default:
			r.selectValues.Set(columns[i], values[i])
		}
	}
	return nil
}

// Value returns the ent.Value that was dynamically selected and assigned to the Repo.
// This includes values selected through modifiers, order, etc.
func (r *Repo) Value(name string) (ent.Value, error) {
	return r.selectValues.Get(name)
}

// QueryRpms queries the "rpms" edge of the Repo entity.
func (r *Repo) QueryRpms() *RpmPackageQuery {
	return NewRepoClient(r.config).QueryRpms(r)
}

// QueryKey queries the "key" edge of the Repo entity.
func (r *Repo) QueryKey() *SigningKeyQuery {
	return NewRepoClient(r.config).QueryKey(r)
}

// Update returns a builder for updating this Repo.
// Note that you need to call Repo.Unwrap() before calling this method if this Repo
// was returned from a transaction, and the transaction was committed or rolled back.
func (r *Repo) Update() *RepoUpdateOne {
	return NewRepoClient(r.config).UpdateOne(r)
}

// Unwrap unwraps the Repo entity that was returned from a transaction after it was closed,
// so that all future queries will be executed through the driver which created the transaction.
func (r *Repo) Unwrap() *Repo {
	_tx, ok := r.config.driver.(*txDriver)
	if !ok {
		panic("ent: Repo is not a transactional entity")
	}
	r.config.driver = _tx.drv
	return r
}

// String implements the fmt.Stringer.
func (r *Repo) String() string {
	var builder strings.Builder
	builder.WriteString("Repo(")
	builder.WriteString(fmt.Sprintf("id=%v, ", r.ID))
	builder.WriteString("type=")
	builder.WriteString(fmt.Sprintf("%v", r.Type))
	builder.WriteByte(')')
	return builder.String()
}

// Repos is a parsable slice of Repo.
type Repos []*Repo
