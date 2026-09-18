package com.acme.billing;

import org.springframework.cloud.openfeign.FeignClient;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;

@FeignClient(name = "accounts")
public interface AccountClient {

    @GetMapping("/accounts/{id}")
    AccountView get(@PathVariable("id") String id);
}
