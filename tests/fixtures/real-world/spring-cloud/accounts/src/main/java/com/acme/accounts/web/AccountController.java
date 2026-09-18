package com.acme.accounts.web;

import com.acme.accounts.client.StatsClient;
import com.acme.accounts.domain.Account;
import org.springframework.amqp.rabbit.core.RabbitTemplate;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class AccountController {

    private final StatsClient stats;
    private final RabbitTemplate rabbitTemplate;

    public AccountController(StatsClient stats, RabbitTemplate rabbitTemplate) {
        this.stats = stats;
        this.rabbitTemplate = rabbitTemplate;
    }

    @GetMapping("/accounts/{name}")
    public Account get(@PathVariable String name) {
        stats.update(name);
        rabbitTemplate.convertAndSend("account.viewed", name);
        return new Account();
    }
}
