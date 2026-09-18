package example.pets;

import java.util.List;
import java.util.Optional;

import io.micronaut.core.annotation.Nullable;
import io.micronaut.http.HttpResponse;
import io.micronaut.http.HttpStatus;
import io.micronaut.http.annotation.*;
import io.micronaut.security.annotation.Secured;
import io.micronaut.security.rules.SecurityRule;
import jakarta.validation.Valid;

@Controller("/pets")
@Secured(SecurityRule.IS_AUTHENTICATED)
public class PetController {

    @Get
    public List<Pet> list(@QueryValue(defaultValue = "10") int max, @Nullable @QueryValue String species) {
        return List.of();
    }

    @Get("/{id}")
    public Optional<Pet> show(Long id) {
        return Optional.empty();
    }

    @Post
    @Status(HttpStatus.CREATED)
    public Pet save(@Body @Valid NewPet pet) {
        return new Pet(1L, pet.name(), pet.species());
    }

    @Delete("/{id}")
    @Secured({"ROLE_ADMIN"})
    public HttpResponse<?> remove(Long id) {
        if (id < 0) {
            return HttpResponse.notFound();
        }
        return HttpResponse.noContent();
    }
}
