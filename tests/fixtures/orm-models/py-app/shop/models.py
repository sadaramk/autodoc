"""Django models for the book shop."""

from django.db import models


class BookStatus(models.TextChoices):
    DRAFT = "draft", "Draft"
    PUBLISHED = "published", "Published"


class Author(models.Model):
    name = models.CharField(max_length=120)


class Book(models.Model):
    author = models.ForeignKey(Author, on_delete=models.CASCADE)
    title = models.CharField(max_length=200, unique=True)
    status = models.CharField(max_length=20, choices=BookStatus.choices, default=BookStatus.DRAFT)
