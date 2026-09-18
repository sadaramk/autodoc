package com.acme.accounts.client;

import org.springframework.cloud.openfeign.FeignClient;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PutMapping;

/** Pushes account changes to the statistics service. */
@FeignClient(name = "statistics-service")
public interface StatsClient {

    @PutMapping("/statistics/{accountName}")
    void update(@PathVariable("accountName") String accountName);
}
