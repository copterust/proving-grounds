from sympy import *
# Our state is [q, b], where q is quatenion and b are biases
state_len = 7
x = IndexedBase('x', shape=(state_len,))
# Without an input our state changed only by biases
# So we repeat our quat
I4 = Identity(4)
# and we assume biases stay the same
I3 = Identity(3)
# we drop the first column to multiply quaternions with 3-vectors
def q2m(q):
    return Matrix([
        [-q.b, -q.c, -q.d],
        [ q.a, -q.d,  q.c],
        [ q.d,  q.a, -q.b],
        [-q.c,  q.b,  q.a]])

# Convert indexed base to list
def i2l(i, start, stop):
    return [i[j] for j in range(start, stop)]

# Convert indexed base to Matrix
def i2m(i):
    s = i.shape
    assert len(s) == 2, "Works only for 2D case"
    m = []
    for r in range(s[0]):
        tr = []
        for c in range(s[1]):
            tr.append(i[r, c])
        m.append(tr)
    return Matrix(m)

# no interaction between q and b
Z3x4 = zeros(3, 4)
# we assume constant angular speed between measurements
dt = symbols("dT")
# Estimated quaternion
q = Quaternion(*i2l(x, 0, 4))
# Estimated bias
b = Matrix([*i2l(x, 4, 6)])
# State transition matrix
A = Matrix(BlockMatrix([[I4, (-dt / 2.0) * q2m(q)], [Z3x4, I3]]))
# Measured angular velocity control our attitude
w = IndexedBase('w', shape=(3,))
w_m = Matrix([*i2l(w, 0, 3)])
# But doesn't influence the bias
Z3x3 = zeros(3, 3)
# So our control
B = BlockMatrix([[q2m(q)], [Z3x3]])
# State in matrix form
x_m = Matrix([x[i] for i in range(state_len)])
# Next state
nx = A * x_m + (dt / 2.0) * Matrix(B) * w_m

def reduce(exp):
    # TODO: why so complicated?
    return simplify(collect(collect(exp, [q0, q1, q2, q3]), dt))

# Output our state transition equation
output = MatrixSymbol('nx', state_len, 1)
print("// State transition")
print(ccode(nx, assign_to=output, contract=False))

# We keep it symbolical for code generation
P = IndexedBase('P', shape=(state_len, state_len))
P_m = i2m(P)
Q = IndexedBase('Q', shape=(state_len, state_len))
Q_m = i2m(Q)

Px = A * P * A.transpose() + Q
output = MatrixSymbol('Px', state_len, state_len)
print("// Error transition")
print(ccode(Px, assign_to=output, contract=False))