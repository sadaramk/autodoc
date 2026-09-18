package com.acme.dao.device;

import java.util.UUID;

public record Device(UUID id, String name, String label) {
}
