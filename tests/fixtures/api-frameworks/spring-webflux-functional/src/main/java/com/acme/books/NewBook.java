package com.acme.books;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record NewBook(@NotBlank @Size(min = 10, max = 13) String isbn, @NotBlank String title) {
}
