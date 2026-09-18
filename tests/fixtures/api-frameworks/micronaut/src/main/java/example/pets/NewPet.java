package example.pets;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Pattern;

public record NewPet(@NotBlank String name, @Pattern(regexp = "cat|dog") String species) {
}
