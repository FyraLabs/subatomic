package property

import "database/sql/driver"

type RepositoryType int

const (
	Dnf RepositoryType = iota
	Ostree
)

func (p RepositoryType) String() string {
	switch p {
	case Dnf:
		return "LOW"
	case Ostree:
		return "HIGH"
	default:
		return ""
	}
}

// Values provides list valid values for Enum.
func (RepositoryType) Values() []string {
	return []string{Dnf.String(), Ostree.String()}
}

// Value provides the DB a string from int.
func (p RepositoryType) Value() (driver.Value, error) {
	return p.String(), nil
}

// Scan tells our code how to read the enum into our type.
func (p *RepositoryType) Scan(val any) error {
	var s string
	switch v := val.(type) {
	case nil:
		return nil
	case string:
		s = v
	case []uint8:
		s = string(v)
	}
	switch s {
	case "LOW":
		*p = Dnf
	case "HIGH":
		*p = Ostree
	}
	return nil
}
