package com.acme.stats.example;

import org.springframework.amqp.rabbit.annotation.RabbitListener;
import org.springframework.stereotype.Component;

/** Lives in a package named `example`: still production code. */
@Component
public class AccountEvents {

    @RabbitListener(queues = "account.viewed")
    public void onViewed(String accountName) {
    }
}
