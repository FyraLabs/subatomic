// Code generated by ent, DO NOT EDIT.

package signingkey

const (
	// Label holds the string label denoting the signingkey type in the database.
	Label = "signing_key"
	// FieldID holds the string denoting the id field in the database.
	FieldID = "oid"
	// FieldPrivateKey holds the string denoting the private_key field in the database.
	FieldPrivateKey = "private_key"
	// FieldPublicKey holds the string denoting the public_key field in the database.
	FieldPublicKey = "public_key"
	// FieldName holds the string denoting the name field in the database.
	FieldName = "name"
	// FieldEmail holds the string denoting the email field in the database.
	FieldEmail = "email"
	// EdgeRepo holds the string denoting the repo edge name in mutations.
	EdgeRepo = "repo"
	// Table holds the table name of the signingkey in the database.
	Table = "signing_keys"
	// RepoTable is the table that holds the repo relation/edge.
	RepoTable = "repos"
	// RepoInverseTable is the table name for the Repo entity.
	// It exists in this package in order to avoid circular dependency with the "repo" package.
	RepoInverseTable = "repos"
	// RepoColumn is the table column denoting the repo relation/edge.
	RepoColumn = "repo_key"
)

// Columns holds all SQL columns for signingkey fields.
var Columns = []string{
	FieldID,
	FieldPrivateKey,
	FieldPublicKey,
	FieldName,
	FieldEmail,
}

// ValidColumn reports if the column name is valid (part of the table columns).
func ValidColumn(column string) bool {
	for i := range Columns {
		if column == Columns[i] {
			return true
		}
	}
	return false
}
