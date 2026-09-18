package com.acme.accounts;

import java.util.List;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class AccountController {

    @GetMapping("/accounts/{id}")
    public AccountView get(@PathVariable("id") String id) {
        return new AccountView(id, "Ada", "12.00");
    }

    @GetMapping("/accounts")
    public List<AccountView> list() {
        return List.of(new AccountView("1", "Ada", "12.00"));
    }
}
