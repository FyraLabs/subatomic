// Code generated by ent, DO NOT EDIT.

package ent

import (
	"fmt"
	"strings"

	"entgo.io/ent"
	"entgo.io/ent/dialect/sql"
	"github.com/FyraLabs/subatomic/server/ent/repo"
	"github.com/FyraLabs/subatomic/server/ent/rpmpackage"
)

// RpmPackage is the model entity for the RpmPackage schema.
type RpmPackage struct {
	config `json:"-"`
	// ID of the ent.
	ID int `json:"id,omitempty"`
	// Name holds the value of the "name" field.
	Name string `json:"name,omitempty"`
	// Epoch holds the value of the "epoch" field.
	Epoch int `json:"epoch,omitempty"`
	// Version holds the value of the "version" field.
	Version string `json:"version,omitempty"`
	// Release holds the value of the "release" field.
	Release string `json:"release,omitempty"`
	// Arch holds the value of the "arch" field.
	Arch string `json:"arch,omitempty"`
	// FilePath holds the value of the "file_path" field.
	FilePath string `json:"file_path,omitempty"`
	// Edges holds the relations/edges for other nodes in the graph.
	// The values are being populated by the RpmPackageQuery when eager-loading is set.
	Edges        RpmPackageEdges `json:"edges"`
	repo_rpms    *string
	selectValues sql.SelectValues
}

// RpmPackageEdges holds the relations/edges for other nodes in the graph.
type RpmPackageEdges struct {
	// Repo holds the value of the repo edge.
	Repo *Repo `json:"repo,omitempty"`
	// loadedTypes holds the information for reporting if a
	// type was loaded (or requested) in eager-loading or not.
	loadedTypes [1]bool
}

// RepoOrErr returns the Repo value or an error if the edge
// was not loaded in eager-loading, or loaded but was not found.
func (e RpmPackageEdges) RepoOrErr() (*Repo, error) {
	if e.Repo != nil {
		return e.Repo, nil
	} else if e.loadedTypes[0] {
		return nil, &NotFoundError{label: repo.Label}
	}
	return nil, &NotLoadedError{edge: "repo"}
}

// scanValues returns the types for scanning values from sql.Rows.
func (*RpmPackage) scanValues(columns []string) ([]any, error) {
	values := make([]any, len(columns))
	for i := range columns {
		switch columns[i] {
		case rpmpackage.FieldID, rpmpackage.FieldEpoch:
			values[i] = new(sql.NullInt64)
		case rpmpackage.FieldName, rpmpackage.FieldVersion, rpmpackage.FieldRelease, rpmpackage.FieldArch, rpmpackage.FieldFilePath:
			values[i] = new(sql.NullString)
		case rpmpackage.ForeignKeys[0]: // repo_rpms
			values[i] = new(sql.NullString)
		default:
			values[i] = new(sql.UnknownType)
		}
	}
	return values, nil
}

// assignValues assigns the values that were returned from sql.Rows (after scanning)
// to the RpmPackage fields.
func (rp *RpmPackage) assignValues(columns []string, values []any) error {
	if m, n := len(values), len(columns); m < n {
		return fmt.Errorf("mismatch number of scan values: %d != %d", m, n)
	}
	for i := range columns {
		switch columns[i] {
		case rpmpackage.FieldID:
			value, ok := values[i].(*sql.NullInt64)
			if !ok {
				return fmt.Errorf("unexpected type %T for field id", value)
			}
			rp.ID = int(value.Int64)
		case rpmpackage.FieldName:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field name", values[i])
			} else if value.Valid {
				rp.Name = value.String
			}
		case rpmpackage.FieldEpoch:
			if value, ok := values[i].(*sql.NullInt64); !ok {
				return fmt.Errorf("unexpected type %T for field epoch", values[i])
			} else if value.Valid {
				rp.Epoch = int(value.Int64)
			}
		case rpmpackage.FieldVersion:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field version", values[i])
			} else if value.Valid {
				rp.Version = value.String
			}
		case rpmpackage.FieldRelease:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field release", values[i])
			} else if value.Valid {
				rp.Release = value.String
			}
		case rpmpackage.FieldArch:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field arch", values[i])
			} else if value.Valid {
				rp.Arch = value.String
			}
		case rpmpackage.FieldFilePath:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field file_path", values[i])
			} else if value.Valid {
				rp.FilePath = value.String
			}
		case rpmpackage.ForeignKeys[0]:
			if value, ok := values[i].(*sql.NullString); !ok {
				return fmt.Errorf("unexpected type %T for field repo_rpms", values[i])
			} else if value.Valid {
				rp.repo_rpms = new(string)
				*rp.repo_rpms = value.String
			}
		default:
			rp.selectValues.Set(columns[i], values[i])
		}
	}
	return nil
}

// Value returns the ent.Value that was dynamically selected and assigned to the RpmPackage.
// This includes values selected through modifiers, order, etc.
func (rp *RpmPackage) Value(name string) (ent.Value, error) {
	return rp.selectValues.Get(name)
}

// QueryRepo queries the "repo" edge of the RpmPackage entity.
func (rp *RpmPackage) QueryRepo() *RepoQuery {
	return NewRpmPackageClient(rp.config).QueryRepo(rp)
}

// Update returns a builder for updating this RpmPackage.
// Note that you need to call RpmPackage.Unwrap() before calling this method if this RpmPackage
// was returned from a transaction, and the transaction was committed or rolled back.
func (rp *RpmPackage) Update() *RpmPackageUpdateOne {
	return NewRpmPackageClient(rp.config).UpdateOne(rp)
}

// Unwrap unwraps the RpmPackage entity that was returned from a transaction after it was closed,
// so that all future queries will be executed through the driver which created the transaction.
func (rp *RpmPackage) Unwrap() *RpmPackage {
	_tx, ok := rp.config.driver.(*txDriver)
	if !ok {
		panic("ent: RpmPackage is not a transactional entity")
	}
	rp.config.driver = _tx.drv
	return rp
}

// String implements the fmt.Stringer.
func (rp *RpmPackage) String() string {
	var builder strings.Builder
	builder.WriteString("RpmPackage(")
	builder.WriteString(fmt.Sprintf("id=%v, ", rp.ID))
	builder.WriteString("name=")
	builder.WriteString(rp.Name)
	builder.WriteString(", ")
	builder.WriteString("epoch=")
	builder.WriteString(fmt.Sprintf("%v", rp.Epoch))
	builder.WriteString(", ")
	builder.WriteString("version=")
	builder.WriteString(rp.Version)
	builder.WriteString(", ")
	builder.WriteString("release=")
	builder.WriteString(rp.Release)
	builder.WriteString(", ")
	builder.WriteString("arch=")
	builder.WriteString(rp.Arch)
	builder.WriteString(", ")
	builder.WriteString("file_path=")
	builder.WriteString(rp.FilePath)
	builder.WriteByte(')')
	return builder.String()
}

// RpmPackages is a parsable slice of RpmPackage.
type RpmPackages []*RpmPackage
