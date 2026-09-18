package com.acme.clinic.domain;

import jakarta.persistence.*;

@Entity
@Table(name = "specialties", uniqueConstraints = @UniqueConstraint(columnNames = "label"))
public class Specialty extends BaseEntity {
    private String label;
}
