package org.acme.fruits;

public class OutOfSeasonException extends RuntimeException {
    public OutOfSeasonException(String message) {
        super(message);
    }
}
