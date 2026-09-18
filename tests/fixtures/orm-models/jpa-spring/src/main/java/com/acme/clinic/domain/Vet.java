package com.acme.clinic.domain;

import jakarta.persistence.*;
import java.util.Set;

@Entity
public class Vet extends BaseEntity {
    @Column(name = "full_name", nullable = false, length = 80)
    private String fullName;

    @ManyToOne
    private Clinic clinic;

    @ManyToMany
    @JoinTable(name = "vet_specialties")
    private Set<Specialty> specialties;

    @OneToMany(mappedBy = "vet")
    private Set<Appointment> appointments;
}
