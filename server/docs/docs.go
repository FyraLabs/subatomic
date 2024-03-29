// Package docs Code generated by swaggo/swag. DO NOT EDIT
package docs

import "github.com/swaggo/swag"

const docTemplate = `{
    "schemes": {{ marshal .Schemes }},
    "swagger": "2.0",
    "info": {
        "description": "{{escape .Description}}",
        "title": "{{.Title}}",
        "contact": {},
        "license": {
            "name": "GPL3",
            "url": "https://choosealicense.com/licenses/gpl-3.0/"
        },
        "version": "{{.Version}}"
    },
    "host": "{{.Host}}",
    "basePath": "{{.BasePath}}",
    "paths": {
        "/keys": {
            "get": {
                "description": "get keys",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "keys"
                ],
                "summary": "Get all keys",
                "responses": {
                    "200": {
                        "description": "OK",
                        "schema": {
                            "type": "array",
                            "items": {
                                "$ref": "#/definitions/types.KeyResponse"
                            }
                        }
                    }
                }
            },
            "post": {
                "description": "create key",
                "consumes": [
                    "application/json"
                ],
                "tags": [
                    "keys"
                ],
                "summary": "Create a new key",
                "parameters": [
                    {
                        "description": "options for the new key",
                        "name": "body",
                        "in": "body",
                        "required": true,
                        "schema": {
                            "$ref": "#/definitions/types.CreateKeyPayload"
                        }
                    }
                ],
                "responses": {
                    "201": {
                        "description": "Created"
                    },
                    "400": {
                        "description": "Bad Request",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    },
                    "409": {
                        "description": "Conflict",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/keys/{id}": {
            "get": {
                "description": "get key",
                "consumes": [
                    "application/json"
                ],
                "tags": [
                    "keys"
                ],
                "summary": "Get a key",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the key",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "200": {
                        "description": "OK"
                    },
                    "400": {
                        "description": "Bad Request",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    },
                    "409": {
                        "description": "Conflict",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/repos": {
            "get": {
                "description": "get repos",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Get all repos",
                "responses": {
                    "200": {
                        "description": "OK",
                        "schema": {
                            "type": "array",
                            "items": {
                                "$ref": "#/definitions/types.RepoResponse"
                            }
                        }
                    }
                }
            },
            "post": {
                "description": "create repo",
                "consumes": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Create a new repo",
                "parameters": [
                    {
                        "description": "options for the new repository",
                        "name": "body",
                        "in": "body",
                        "required": true,
                        "schema": {
                            "$ref": "#/definitions/types.CreateRepoPayload"
                        }
                    }
                ],
                "responses": {
                    "200": {
                        "description": "OK"
                    },
                    "400": {
                        "description": "Bad Request",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    },
                    "409": {
                        "description": "Conflict",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/repos/{id}": {
            "put": {
                "description": "upload to repo",
                "consumes": [
                    "multipart/form-data"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Upload files to a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    },
                    {
                        "type": "string",
                        "description": "files to upload to this reposiutory",
                        "name": "file_upload",
                        "in": "formData",
                        "required": true
                    }
                ],
                "responses": {
                    "200": {
                        "description": "OK"
                    },
                    "400": {
                        "description": "Bad Request",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            },
            "delete": {
                "description": "delete repo",
                "tags": [
                    "repos"
                ],
                "summary": "Delete a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "200": {
                        "description": "OK"
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/repos/{id}/comps": {
            "put": {
                "description": "push rpm comps",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Push a RPM comps file",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "204": {
                        "description": "No Content"
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            },
            "delete": {
                "description": "delete4 rpm comps",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Delete the RPM comps file",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "204": {
                        "description": "No Content"
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/repos/{id}/key": {
            "get": {
                "description": "get repo key",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Get key for a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "200": {
                        "description": "OK",
                        "schema": {
                            "$ref": "#/definitions/main.fullKeyResponse"
                        }
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            },
            "put": {
                "description": "set repo key",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Set key for a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    },
                    {
                        "description": "options for the key to set",
                        "name": "body",
                        "in": "body",
                        "required": true,
                        "schema": {
                            "$ref": "#/definitions/types.SetKeyPayload"
                        }
                    }
                ],
                "responses": {
                    "204": {
                        "description": "No Content"
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            },
            "delete": {
                "description": "delete repo key",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Delete key for a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "204": {
                        "description": "No Content"
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/repos/{id}/resign": {
            "post": {
                "description": "resign repo packages",
                "produces": [
                    "application/json"
                ],
                "tags": [
                    "repos"
                ],
                "summary": "Resign packages in a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "204": {
                        "description": "No Content"
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/repos/{id}/rpms": {
            "get": {
                "description": "rpms in repo",
                "tags": [
                    "repos"
                ],
                "summary": "Get list of RPMs in a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "200": {
                        "description": "OK",
                        "schema": {
                            "type": "array",
                            "items": {
                                "$ref": "#/definitions/types.RpmResponse"
                            }
                        }
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        },
        "/repos/{id}/rpms/{rpmID}": {
            "delete": {
                "description": "delete rpm",
                "tags": [
                    "repos"
                ],
                "summary": "Delete RPM in a repo",
                "parameters": [
                    {
                        "type": "string",
                        "description": "id for the repository",
                        "name": "id",
                        "in": "path",
                        "required": true
                    },
                    {
                        "type": "string",
                        "description": "rpm id in the repository",
                        "name": "rpmID",
                        "in": "path",
                        "required": true
                    }
                ],
                "responses": {
                    "200": {
                        "description": "OK"
                    },
                    "404": {
                        "description": "Not Found",
                        "schema": {
                            "$ref": "#/definitions/types.ErrResponse"
                        }
                    }
                }
            }
        }
    },
    "definitions": {
        "main.fullKeyResponse": {
            "type": "object",
            "properties": {
                "email": {
                    "type": "string"
                },
                "id": {
                    "type": "string"
                },
                "name": {
                    "type": "string"
                },
                "public_key": {
                    "type": "string"
                }
            }
        },
        "types.CreateKeyPayload": {
            "type": "object",
            "required": [
                "email",
                "id",
                "name"
            ],
            "properties": {
                "email": {
                    "type": "string"
                },
                "id": {
                    "type": "string"
                },
                "name": {
                    "type": "string"
                }
            }
        },
        "types.CreateRepoPayload": {
            "type": "object",
            "required": [
                "id",
                "type"
            ],
            "properties": {
                "id": {
                    "type": "string"
                },
                "type": {
                    "type": "string",
                    "enum": [
                        "rpm"
                    ]
                }
            }
        },
        "types.ErrResponse": {
            "type": "object",
            "properties": {
                "code": {
                    "description": "application-specific error code",
                    "type": "integer"
                },
                "error": {
                    "description": "application-level error message, for debugging",
                    "type": "string"
                },
                "status": {
                    "description": "user-level status message",
                    "type": "string"
                }
            }
        },
        "types.KeyResponse": {
            "type": "object",
            "properties": {
                "email": {
                    "type": "string"
                },
                "id": {
                    "type": "string"
                },
                "name": {
                    "type": "string"
                }
            }
        },
        "types.RepoResponse": {
            "type": "object",
            "properties": {
                "id": {
                    "type": "string"
                },
                "type": {
                    "type": "string"
                }
            }
        },
        "types.RpmResponse": {
            "type": "object",
            "properties": {
                "arch": {
                    "type": "string"
                },
                "epoch": {
                    "type": "integer"
                },
                "file_path": {
                    "type": "string"
                },
                "id": {
                    "type": "integer"
                },
                "name": {
                    "type": "string"
                },
                "release": {
                    "type": "string"
                },
                "version": {
                    "type": "string"
                }
            }
        },
        "types.SetKeyPayload": {
            "type": "object",
            "required": [
                "id"
            ],
            "properties": {
                "id": {
                    "type": "string"
                }
            }
        }
    },
    "securityDefinitions": {
        "": {
            "type": "apiKey",
            "name": "Authorization",
            "in": "header"
        }
    }
}`

// SwaggerInfo holds exported Swagger Info so clients can modify it
var SwaggerInfo = &swag.Spec{
	Version:          "1.0",
	Host:             "",
	BasePath:         "/",
	Schemes:          []string{},
	Title:            "Subatomic",
	Description:      "A modern package delivery server.",
	InfoInstanceName: "swagger",
	SwaggerTemplate:  docTemplate,
	LeftDelim:        "{{",
	RightDelim:       "}}",
}

func init() {
	swag.Register(SwaggerInfo.InstanceName(), SwaggerInfo)
}
