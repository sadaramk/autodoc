package com.acme.clinic.service;

import com.acme.clinic.domain.*;
import com.acme.clinic.repo.*;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
@RequiredArgsConstructor
public class AppointmentService {
    private final AppointmentRepository appointments;
    private final VetRepository vets;

    @Transactional
    public Appointment book(Long vetId) {
        Vet vet = vets.findById(vetId).orElseThrow();
        Appointment appointment = new Appointment();
        appointment.setVet(vet);
        return appointments.save(appointment);
    }

    @Transactional
    public void confirm(Long id) {
        Appointment appointment = appointments.findById(id).orElseThrow();
        if (appointment.getStatus() != AppointmentStatus.REQUESTED) {
            throw new IllegalStateException("only requested appointments can be confirmed");
        }
        appointment.setStatus(AppointmentStatus.CONFIRMED);
    }

    @Transactional
    public void complete(Long id) {
        Appointment appointment = appointments.findById(id).orElseThrow();
        if (!appointment.getStatus().equals(AppointmentStatus.CONFIRMED)) {
            throw new IllegalStateException("not confirmed");
        }
        appointment.setStatus(AppointmentStatus.COMPLETED);
    }

    @Transactional
    public void cancel(Long id) {
        Appointment appointment = appointments.findById(id).orElseThrow();
        appointment.setStatus(AppointmentStatus.CANCELLED);
    }

    public int cleanup(java.time.Instant cutoff) {
        return appointments.cancelStale(cutoff);
    }
}
