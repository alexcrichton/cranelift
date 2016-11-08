"""
Abstract syntax trees.

This module defines classes that can be used to create abstract syntax trees
for patern matching an rewriting of cretonne instructions.
"""
from __future__ import absolute_import
import cretonne

try:
    from typing import Union, Tuple  # noqa
except ImportError:
    pass


class Def(object):
    """
    An AST definition associates a set of variables with the values produced by
    an expression.

    Example:

    >>> from .base import iadd_cout, iconst
    >>> x = Var('x')
    >>> y = Var('y')
    >>> x << iconst(4)
    (Var(x),) << Apply(iconst, (4,))
    >>> (x, y) << iadd_cout(4, 5)
    (Var(x), Var(y)) << Apply(iadd_cout, (4, 5))

    The `<<` operator is used to create variable definitions.

    :param defs: Single variable or tuple of variables to be defined.
    :param expr: Expression generating the values.
    """

    def __init__(self, defs, expr):
        # type: (Union[Var, Tuple[Var, ...]], Apply) -> None
        if not isinstance(defs, tuple):
            self.defs = (defs,)  # type: Tuple[Var, ...]
        else:
            self.defs = defs
        assert isinstance(expr, Apply)
        self.expr = expr

    def __repr__(self):
        return "{} << {!r}".format(self.defs, self.expr)

    def __str__(self):
        if len(self.defs) == 1:
            return "{!s} << {!s}".format(self.defs[0], self.expr)
        else:
            return "({}) << {!s}".format(
                    ', '.join(map(str, self.defs)), self.expr)


class Expr(object):
    """
    An AST expression.
    """


class Var(Expr):
    """
    A free variable.

    When variables are used in `XForms` with source and destination patterns,
    they are classified as follows:

    Input values
        Uses in the source pattern with no preceding def. These may appear as
        inputs in the destination pattern too, but no new inputs can be
        introduced.
    Output values
        Variables that are defined in both the source and destination pattern.
        These values may have uses outside the source pattern, and the
        destination pattern must compute the same value.
    Intermediate values
        Values that are defined in the source pattern, but not in the
        destination pattern. These may have uses outside the source pattern, so
        the defining instruction can't be deleted immediately.
    Temporary values
        Values that are defined only in the destination pattern.
    """

    def __init__(self, name):
        # type: (str) -> None
        self.name = name
        # The `Def` defining this variable in a source pattern.
        self.src_def = None  # type: Def
        # The `Def` defining this variable in a destination pattern.
        self.dst_def = None  # type: Def

    def __str__(self):
        # type: () -> str
        return self.name

    def __repr__(self):
        # type: () -> str
        s = self.name
        if self.src_def:
            s += ", src"
        if self.dst_def:
            s += ", dst"
        return "Var({})".format(s)

    # Context bits for `set_def` indicating which pattern has defines of this
    # var.
    SRCCTX = 1
    DSTCTX = 2

    def set_def(self, context, d):
        # type: (int, Def) -> None
        """
        Set the `Def` that defines this variable in the given context.

        The `context` must be one of `SRCCTX` or `DSTCTX`
        """
        if context == self.SRCCTX:
            self.src_def = d
        else:
            self.dst_def = d

    def get_def(self, context):
        # type: (int) -> Def
        """
        Get the def of this variable in context.

        The `context` must be one of `SRCCTX` or `DSTCTX`
        """
        if context == self.SRCCTX:
            return self.src_def
        else:
            return self.dst_def

    def is_input(self):
        # type: () -> bool
        """Is this an input value to the src pattern?"""
        return not self.src_def and not self.dst_def

    def is_output(self):
        """Is this an output value, defined in both src and dst patterns?"""
        # type: () -> bool
        return self.src_def and self.dst_def

    def is_intermediate(self):
        """Is this an intermediate value, defined only in the src pattern?"""
        # type: () -> bool
        return self.src_def and not self.dst_def

    def is_temp(self):
        """Is this a temp value, defined only in the dst pattern?"""
        # type: () -> bool
        return not self.src_def and self.dst_def


class Apply(Expr):
    """
    Apply an instruction to arguments.

    An `Apply` AST expression is created by using function call syntax on
    instructions. This applies to both bound and unbound polymorphic
    instructions:

    >>> from .base import jump, iadd
    >>> jump('next', ())
    Apply(jump, ('next', ()))
    >>> iadd.i32('x', 'y')
    Apply(iadd.i32, ('x', 'y'))

    :param inst: The instruction being applied, an `Instruction` or
                 `BoundInstruction` instance.
    :param args: Tuple of arguments.
    """

    def __init__(self, inst, args):
        # type: (Union[cretonne.Instruction, cretonne.BoundInstruction], Tuple[Expr, ...]) -> None  # noqa
        if isinstance(inst, cretonne.BoundInstruction):
            self.inst = inst.inst
            self.typevars = inst.typevars
        else:
            assert isinstance(inst, cretonne.Instruction)
            self.inst = inst
            self.typevars = ()
        self.args = args
        assert len(self.inst.ins) == len(args)

    def __rlshift__(self, other):
        # type: (Union[Var, Tuple[Var, ...]]) -> Def
        """
        Define variables using `var << expr` or `(v1, v2) << expr`.
        """
        return Def(other, self)

    def instname(self):
        i = self.inst.name
        for t in self.typevars:
            i += '.{}'.format(t)
        return i

    def __repr__(self):
        return "Apply({}, {})".format(self.instname(), self.args)

    def __str__(self):
        args = ', '.join(map(str, self.args))
        return '{}({})'.format(self.instname(), args)

    def rust_builder(self):
        # type: () -> str
        """
        Return a Rust Builder method call for instantiating this instruction
        application.
        """
        args = ', '.join(map(str, self.args))
        method = self.inst.snake_name()
        return '{}({})'.format(method, args)
