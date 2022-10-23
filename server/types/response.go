package types

type RepoResponse struct {
	ID   string `json:"id"`
	Type string `json:"type"`
}

// TODO: maybe we could add support for other package types
type RpmResponse struct {
	ID       int    `json:"id"`
	Name     string `json:"name"`
	Epoch    int    `json:"epoch"`
	Version  string `json:"version"`
	Release  string `json:"release"`
	Arch     string `json:"arch"`
	FilePath string `json:"file_path"`
}
