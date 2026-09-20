"""A billing statement."""

from django.db import models


class Statement(models.Model):
    """One statement issued to a customer."""

    reference = models.CharField(max_length=64, unique=True)
    total_cents = models.IntegerField()
