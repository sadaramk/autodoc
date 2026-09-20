package com.acme.gateway;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.cloud.openfeign.FeignClient;
import org.springframework.web.bind.annotation.GetMapping;

/** The gateway, which calls the accounts service by its configured name. */
@SpringBootApplication
public class GatewayApplication {
    public static void main(String[] args) {
        SpringApplication.run(GatewayApplication.class, args);
    }
}

/** Reaches the accounts service. */
@FeignClient(name = "account-service")
interface AccountClient {
    /** Fetches one account. */
    @GetMapping("/accounts/{id}")
    String findById(String id);
}
