package org.acme.fruits;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.PositiveOrZero;
import jakarta.validation.constraints.Size;

public record NewFruit(@NotBlank @Size(max = 40) String name, Season season, @PositiveOrZero int stock) {
}
