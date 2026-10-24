package main

type T struct{ A int `json:"a"` }

type User struct {
	ID        int64     `json:"id" db:"id,pk"`
	Name      string    `json:"name,omitempty" validate:"required"`
	Email     *string   `json:"email"`
	CreatedAt time.Time `json:"created_at"`
	Tags      []string  "json:\"tags\""
	Nested    struct {
		Inner bool `json:"inner"`
	} `json:"nested"`
	Plain int
	embedded
}

type embedded struct {
	X, Y float64 `xml:"x,attr"`
}
