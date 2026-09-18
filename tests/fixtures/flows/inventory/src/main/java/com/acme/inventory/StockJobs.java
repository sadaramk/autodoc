package com.acme.inventory;

import org.springframework.context.ApplicationEventPublisher;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.mail.javamail.JavaMailSender;
import org.springframework.mail.SimpleMailMessage;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

@Component
public class StockJobs {

    private final JdbcTemplate jdbc;
    private final ApplicationEventPublisher eventPublisher;
    private final JavaMailSender mailSender;

    public StockJobs(JdbcTemplate jdbc, ApplicationEventPublisher eventPublisher, JavaMailSender mailSender) {
        this.jdbc = jdbc;
        this.eventPublisher = eventPublisher;
        this.mailSender = mailSender;
    }

    /** Expires reservations on the schedule the configuration sets. */
    @Scheduled(cron = "${stock.expiry.cron}")
    public void expire() {
        jdbc.update("UPDATE stock SET reserved = 0 WHERE expires_at < now()");
        eventPublisher.publishEvent(StockExpired.builder().sku("*").build());
        SimpleMailMessage warning = new SimpleMailMessage();
        mailSender.send(warning);
    }

    /** Releases reservations nobody paid for. */
    @Scheduled(cron = "0 0 * * * *")
    public void reconcile() {
        jdbc.update("UPDATE stock SET reserved = 0 WHERE reserved < 0");
    }
}
