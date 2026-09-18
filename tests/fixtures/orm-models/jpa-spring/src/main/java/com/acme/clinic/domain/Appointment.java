package com.acme.clinic.domain;

import jakarta.persistence.*;
import jakarta.validation.constraints.*;
import java.math.BigDecimal;

@Entity
@Table(name = "appointments")
public class Appointment extends BaseEntity {
    @ManyToOne(optional = false)
    @JoinColumn(name = "vet_id")
    private Vet vet;

    @Column(name = "pet_name", nullable = false, length = 60)
    private String petName;

    @NotNull
    @Size(max = 500)
    private String reason;

    @Min(15)
    @Max(120)
    private int durationMinutes;

    @Column(precision = 10, scale = 2)
    private BigDecimal fee;

    @Enumerated(EnumType.STRING)
    private AppointmentStatus status = AppointmentStatus.REQUESTED;

    @Transient
    private String displayLabel;

    public AppointmentStatus getStatus() { return status; }
    public void setStatus(AppointmentStatus status) { this.status = status; }
    public void setVet(Vet vet) { this.vet = vet; }
}
