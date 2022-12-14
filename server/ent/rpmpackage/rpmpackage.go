// Code generated by ent, DO NOT EDIT.

package rpmpackage

const (
	// Label holds the string label denoting the rpmpackage type in the database.
	Label = "rpm_package"
	// FieldID holds the string denoting the id field in the database.
	FieldID = "id"
	// FieldName holds the string denoting the name field in the database.
	FieldName = "name"
	// FieldEpoch holds the string denoting the epoch field in the database.
	FieldEpoch = "epoch"
	// FieldVersion holds the string denoting the version field in the database.
	FieldVersion = "version"
	// FieldRelease holds the string denoting the release field in the database.
	FieldRelease = "release"
	// FieldArch holds the string denoting the arch field in the database.
	FieldArch = "arch"
	// FieldFilePath holds the string denoting the file_path field in the database.
	FieldFilePath = "file_path"
	// EdgeRepo holds the string denoting the repo edge name in mutations.
	EdgeRepo = "repo"
	// RepoFieldID holds the string denoting the ID field of the Repo.
	RepoFieldID = "oid"
	// Table holds the table name of the rpmpackage in the database.
	Table = "rpm_packages"
	// RepoTable is the table that holds the repo relation/edge.
	RepoTable = "rpm_packages"
	// RepoInverseTable is the table name for the Repo entity.
	// It exists in this package in order to avoid circular dependency with the "repo" package.
	RepoInverseTable = "repos"
	// RepoColumn is the table column denoting the repo relation/edge.
	RepoColumn = "repo_rpms"
)

// Columns holds all SQL columns for rpmpackage fields.
var Columns = []string{
	FieldID,
	FieldName,
	FieldEpoch,
	FieldVersion,
	FieldRelease,
	FieldArch,
	FieldFilePath,
}

// ForeignKeys holds the SQL foreign-keys that are owned by the "rpm_packages"
// table and are not defined as standalone fields in the schema.
var ForeignKeys = []string{
	"repo_rpms",
}

// ValidColumn reports if the column name is valid (part of the table columns).
func ValidColumn(column string) bool {
	for i := range Columns {
		if column == Columns[i] {
			return true
		}
	}
	for i := range ForeignKeys {
		if column == ForeignKeys[i] {
			return true
		}
	}
	return false
}

var (
	// EpochValidator is a validator for the "epoch" field. It is called by the builders before save.
	EpochValidator func(int) error
)
