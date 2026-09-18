"""Book publishing."""

from shop.models import Book, BookStatus


def publish(book_id: int) -> None:
    """Publish a draft book."""
    book = Book.objects.get(pk=book_id)
    if book.status != BookStatus.DRAFT:
        raise ValueError("already published")
    book.status = BookStatus.PUBLISHED
    book.save()
