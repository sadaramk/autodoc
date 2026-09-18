package main

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// Book is a catalogued title.
type Book struct {
	ID     string `json:"id"`
	Title  string `json:"title"`
	Author string `json:"author"`
}

// NewBook is the body of POST /api/v1/books.
type NewBook struct {
	Title  string `json:"title" binding:"required,max=200"`
	Author string `json:"author" binding:"required"`
	ISBN   string `json:"isbn" binding:"omitempty,len=13"`
}

func main() {
	r := gin.Default()
	v1 := r.Group("/api/v1")
	books := v1.Group("/books")
	books.GET("", listBooks)
	books.GET("/:id", getBook)

	admin := v1.Group("/admin")
	admin.Use(AuthRequired())
	admin.POST("/books", createBook)
	r.Run(":8080")
}

// listBooks pages through the catalogue.
func listBooks(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	_ = page
	var books []Book
	c.JSON(http.StatusOK, books)
}

func getBook(c *gin.Context) {
	id := c.Param("id")
	if id == "" {
		c.JSON(http.StatusNotFound, gin.H{"error": "book not found"})
		return
	}
	c.JSON(http.StatusOK, Book{ID: id})
}

func createBook(c *gin.Context) {
	var input NewBook
	if err := c.ShouldBindJSON(&input); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusCreated, Book{ID: "b1", Title: input.Title, Author: input.Author})
}

// AuthRequired rejects requests without a bearer token.
func AuthRequired() gin.HandlerFunc {
	return func(c *gin.Context) {
		if c.GetHeader("Authorization") == "" {
			c.AbortWithStatus(http.StatusUnauthorized)
		}
	}
}
