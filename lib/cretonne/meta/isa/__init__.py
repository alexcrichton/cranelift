"""
Cretonne target ISA definitions
-------------------------------

The :py:mod:`isa` package contains sub-packages for each target instruction set
architecture supported by Cretonne.
"""
from __future__ import absolute_import
from cdsl.isa import TargetISA  # noqa
from . import riscv


def all_isas():
    # type: () -> List[TargetISA]
    """
    Get a list of all the supported target ISAs. Each target ISA is represented
    as a :py:class:`cretonne.TargetISA` instance.
    """
    return [riscv.ISA]
