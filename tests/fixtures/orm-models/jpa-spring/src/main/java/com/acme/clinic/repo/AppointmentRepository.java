package com.acme.clinic.repo;

import com.acme.clinic.domain.Appointment;
import org.springframework.data.jpa.repository.*;
import java.util.List;

public interface AppointmentRepository extends JpaRepository<Appointment, Long> {
    @Query("select a from Appointment a where a.vet.id = :vetId")
    List<Appointment> forVet(Long vetId);

    @Modifying
    @Query("update Appointment a set a.status = 'CANCELLED' where a.createdAt < :cutoff")
    int cancelStale(java.time.Instant cutoff);
}
