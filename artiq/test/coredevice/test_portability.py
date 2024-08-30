import unittest
from operator import itemgetter

from numpy import int32, int64

from artiq.coredevice.core import Core
from artiq.experiment import *
from artiq.sim import devices as sim_devices
from artiq.test.hardware_testbench import ExperimentCase


def _run_on_host(k_class, *args, **kwargs):
    device_mgr = dict()
    device_mgr["core"] = sim_devices.Core(device_mgr)

    k_inst = k_class((device_mgr, None, None, {}),
                     *args, **kwargs)
    k_inst.run()
    return k_inst


@nac3
class _Primes(EnvExperiment):
    core: KernelInvariant[Core]
    maximum: KernelInvariant[int32]

    def build(self, output_list, maximum):
        self.setattr_device("core")
        self.output_list = output_list
        self.maximum = maximum

    @rpc
    def _add_output(self, x: int32):
        self.output_list.append(x)

    @kernel
    def run(self):
        for x in range(1, self.maximum):
            d = 2
            prime = True
            while d*d <= x:
                if x % d == 0:
                    prime = False
                    break
                d += 1
            if prime:
                self._add_output(x)


@nac3
class _Math(EnvExperiment):
    core: KernelInvariant[Core]
    x: KernelInvariant[float]
    x_sqrt: Kernel[float]

    def build(self):
        self.setattr_device("core")
        self.x = 3.1
        self.x_sqrt = 0.0

    @kernel
    def run(self):
        self.x_sqrt = self.x**0.5


@nac3
class _Misc(EnvExperiment):
    core: KernelInvariant[Core]

    input: KernelInvariant[int32]
    al: KernelInvariant[list[int32]]
    list_copy_in: KernelInvariant[list[float]]

    half_input: Kernel[int32]
    acc: Kernel[int32]
    list_copy_out: Kernel[list[float]]

    def build(self):
        self.setattr_device("core")

        self.input = 84
        self.al = [1, 2, 3, 4, 5]
        self.list_copy_in = [2*Hz, 10*MHz]

        self.half_input = 0
        self.acc = 0
        self.list_copy_out = []

    @kernel
    def run(self):
        self.half_input = self.input//2
        self.acc = 0
        for i in range(len(self.al)):
            self.acc += self.al[i]
        self.list_copy_out = self.list_copy_in


@nac3
class _PulseLogger(EnvExperiment):
    core: KernelInvariant[Core]

    def build(self, parent_test, name):
        self.setattr_device("core")
        self.parent_test = parent_test
        self.name = name

    def _append(self, t, l, f):
        if not hasattr(self.parent_test, "first_timestamp"):
            self.parent_test.first_timestamp = t
        origin = self.parent_test.first_timestamp
        t_usec = round(self.core.mu_to_seconds(t-origin)*1000000)
        self.parent_test.output_list.append((self.name, t_usec, l, f))

    @rpc
    def on(self, t: int64, f: int32):
        self._append(t, True, f)

    @rpc
    def off(self, t: int64):
        self._append(t, False, 0)

    @kernel
    def pulse(self, f: int32, duration: float):
        self.on(now_mu(), f)
        self.core.delay(duration)
        self.off(now_mu())


@nac3
class _Pulses(EnvExperiment):
    core: KernelInvariant[Core]
    a: KernelInvariant[_PulseLogger]
    b: KernelInvariant[_PulseLogger]
    c: KernelInvariant[_PulseLogger]
    d: KernelInvariant[_PulseLogger]

    def build(self, output_list):
        self.setattr_device("core")
        self.output_list = output_list

        for name in "a", "b", "c", "d":
            pl = _PulseLogger(self,
                              parent_test=self,
                              name=name)
            setattr(self, name, pl)

    @kernel
    def run(self):
        for i in range(3):
            with parallel:
                with sequential:
                    self.a.pulse(100+i, 20.*us)
                    self.b.pulse(200+i, 20.*us)
                with sequential:
                    self.c.pulse(300+i, 10.*us)
                    self.d.pulse(400+i, 20.*us)


@nac3
class _MyException(Exception):
    pass


@nac3
class _NestedFinally(EnvExperiment):
    core: KernelInvariant[Core]

    def build(self, trace):
        self.setattr_device("core")
        self.trace = trace

    @rpc
    def _trace(self, i: int32):
        self.trace.append(i)

    @kernel
    def run(self):
        try:
            try:
                raise ValueError
            finally:
                try:
                    raise IndexError
                except ValueError:
                    self._trace(0)
        except:
            self._trace(1)
        finally:
            self._trace(2)


@nac3
class _NestedExceptions(EnvExperiment):
    core: KernelInvariant[Core]

    def build(self, trace):
        self.setattr_device("core")
        self.trace = trace

    @rpc
    def _trace(self, i: int32):
        self.trace.append(i)

    @kernel
    def run(self):
        try:
            try:
                raise ValueError
            except _MyException:
                self._trace(0)
                raise
            finally:
                try:
                    raise IndexError
                except ValueError:
                    self._trace(1)
                    raise
        except IndexError:
            self._trace(2)
        except:
            self._trace(3)
        finally:
            self._trace(4)


@nac3
class _Exceptions(EnvExperiment):
    core: KernelInvariant[Core]

    def build(self, trace):
        self.setattr_device("core")
        self.trace = trace

    @rpc
    def _trace(self, i: int32):
        self.trace.append(i)

    @kernel
    def run(self):
        for i in range(10):
            self._trace(i)
            if i == 4:
                try:
                    self._trace(10)
                    try:
                        self._trace(11)
                        break
                    except:
                        pass
                    else:
                        self._trace(12)
                    try:
                        self._trace(13)
                    except:
                        pass
                except _MyException:
                    self._trace(14)

        for i in range(4):
            try:
                self._trace(100)
                if i == 1:
                    raise _MyException()
                elif i == 2:
                    raise IndexError
            except IndexError:
                self._trace(101)
                raise
            except:
                self._trace(102)
            else:
                self._trace(103)
            finally:
                self._trace(104)


@nac3
class _RPCExceptions(EnvExperiment):
    core: KernelInvariant[Core]
    catch: KernelInvariant[bool]
    success: Kernel[bool]

    def build(self, catch):
        self.setattr_device("core")
        self.catch = catch

        self.success = False

    @rpc
    def exception_raiser(self):
        raise _MyException

    @kernel
    def run(self):
        if self.catch:
            self.do_catch()
        else:
            self.do_not_catch()

    @kernel
    def do_not_catch(self):
        self.exception_raiser()

    @kernel
    def do_catch(self):
        try:
            self.exception_raiser()
        except _MyException:
            self.success = True


@nac3
class _Keywords(EnvExperiment):
    core: KernelInvariant[Core]
    value: KernelInvariant[int32]

    def build(self, value, output):
        self.setattr_device("core")
        self.value = value
        self.output = output

    @rpc
    def rpc(self, kw: int32):
        self.output.append(kw)

    @kernel
    def run(self):
        self.rpc(kw=self.value)


class HostVsDeviceCase(ExperimentCase):
    def test_primes(self):
        l_device, l_host = [], []
        self.execute(_Primes, maximum=100, output_list=l_device)
        _run_on_host(_Primes, maximum=100, output_list=l_host)
        self.assertEqual(l_device, l_host)

    def test_math(self):
        math_device = self.execute(_Math)
        math_host = _run_on_host(_Math)
        self.assertEqual(math_device.x_sqrt, math_host.x_sqrt)

    def test_misc(self):
        for f in self.execute, _run_on_host:
            uut = f(_Misc)
            self.assertEqual(uut.half_input, 42)
            self.assertEqual(uut.acc, sum(uut.al))
            self.assertEqual(uut.list_copy_in, uut.list_copy_out)

    def test_pulses(self):
        l_device, l_host = [], []
        self.execute(_Pulses, output_list=l_device)
        _run_on_host(_Pulses, output_list=l_host)
        l_host = sorted(l_host, key=itemgetter(1))
        for channel in "a", "b", "c", "d":
            c_device = [x for x in l_device if x[0] == channel]
            c_host = [x for x in l_host if x[0] == channel]
            self.assertEqual(c_device, c_host)

    def test_exceptions(self):
        t_device, t_host = [], []
        with self.assertRaises(IndexError):
            self.execute(_Exceptions, trace=t_device)
        with self.assertRaises(IndexError):
            _run_on_host(_Exceptions, trace=t_host)
        self.assertEqual(t_device, t_host)

    def test_nested_finally(self):
        t_device, t_host = [], []
        self.execute(_NestedFinally, trace=t_device)
        _run_on_host(_NestedFinally, trace=t_host)
        self.assertEqual(t_device, t_host)

    def test_nested_exceptions(self):
        t_device, t_host = [], []
        self.execute(_NestedExceptions, trace=t_device)
        _run_on_host(_NestedExceptions, trace=t_host)
        self.assertEqual(t_device, t_host)

    def test_rpc_exceptions(self):
        for f in self.execute, _run_on_host:
            with self.assertRaises(_MyException):
                f(_RPCExceptions, catch=False)
            uut = self.execute(_RPCExceptions, catch=True)
            self.assertTrue(uut.success)

    @unittest.skip("NAC3TODO https://git.m-labs.hk/M-Labs/nac3/issues/533")
    def test_keywords(self):
        for f in self.execute, _run_on_host:
            output = []
            f(_Keywords, value=0, output=output)
            self.assertEqual(output, [0])
            output = []
            f(_Keywords, value=1, output=output)
            self.assertEqual(output, [1])
