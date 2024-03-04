package tetsudou

import (
	"crypto/md5"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/sha512"
	"fmt"
	"io"
	"os"
)

type Hashes struct {
	Md5    string `json:"md5"`
	Sha1   string `json:"sha1"`
	Sha256 string `json:"sha256"`
	Sha512 string `json:"sha512"`
}

func HashesFromReader(r io.Reader) (Hashes, error) {
	md5hash := md5.New()
	sha1hash := sha1.New()
	sha256hash := sha256.New()
	sha512hash := sha512.New()

	if _, err := io.Copy(io.MultiWriter(md5hash, sha1hash, sha256hash, sha512hash), r); err != nil {
		return Hashes{}, err
	}

	return Hashes{
		Md5:    fmt.Sprintf("%x", md5hash.Sum(nil)),
		Sha1:   fmt.Sprintf("%x", sha1hash.Sum(nil)),
		Sha256: fmt.Sprintf("%x", sha256hash.Sum(nil)),
		Sha512: fmt.Sprintf("%x", sha512hash.Sum(nil)),
	}, nil
}

type Repodata struct {
	Timestamp int64  `json:"timestamp"`
	Size      int64  `json:"size"`
	Hashes    Hashes `json:"hashes"`
}

func RepodataFromFile(f *os.File) (Repodata, error) {
	hashes, err := HashesFromReader(f)
	if err != nil {
		return Repodata{}, err
	}

	fi, err := f.Stat()
	if err != nil {
		return Repodata{}, err
	}

	return Repodata{
		// we use the last modified time as the timestamp.. at least I think that's correct for metalink?
		fi.ModTime().Unix(),
		fi.Size(),
		hashes,
	}, nil
}
