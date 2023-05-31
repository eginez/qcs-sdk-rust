/// Spawn and block on a future using the pyo3 tokio runtime.
/// Useful for returning a synchronous `PyResult`.
///
///
/// When used like the following:
/// ```rs
/// async fn say_hello(name: String) -> String {
///     format!("hello {name}")
/// }
///
/// #[pyo3(name="say_hello")]
/// pub fn py_say_hello(name: String) -> PyResult<String> {
///     py_sync!(say_hello(name))
/// }
/// ```
///
/// Becomes the associated "synchronous" python call:
/// ```py
/// assert say_hello("Rigetti") == "hello Rigetti"
/// ```
macro_rules! py_sync {
    ($py: ident, $body: expr) => {{
        let runtime = ::pyo3_asyncio::tokio::get_runtime();
        let handle = runtime.spawn($body);

        // A 100ms loop delay is a bit arbitrary, but seems to
        // balance CPU usage and SIGINT responsiveness well enough.
        let delay = ::std::time::Duration::from_millis(100);
        while !handle.is_finished() {
            $py.check_signals()?;
            ::std::thread::sleep(delay);
        }

        runtime
            .block_on(handle)
            .map_err(|err| ::pyo3::exceptions::PyRuntimeError::new_err(err.to_string()))?
    }};
}

/// Convert a rust future into a Python awaitable using
/// `pyo3_asyncio::tokio::future_into_py`
macro_rules! py_async {
    ($py: ident, $body: expr) => {
        ::pyo3_asyncio::tokio::future_into_py($py, $body)
    };
}

/// Given a single implementation of an async function,
/// create that function as private and two pyfunctions
/// named after it that can be used to invoke either
/// blocking or async variants of the same function.
///
/// This macro cannot be used when lifetime specifiers are
/// required, or the pyfunction bodies need additional
/// parameter handling besides simply calling out to
/// the underlying `py_async` or `py_sync` macros.
///
/// ```rs
/// // ... becomes python package "things"
/// create_init_submodule! {
///     funcs: [
///         py_do_thing,
///         py_do_thing_async,
///     ]
/// }
///
/// py_function_sync_async! {
///     #[args(timeout = "None")]
///     async fn do_thing(timeout: Option<u64>) -> PyResult<String> {
///         // ... sleep for timeout ...
///         Ok(String::from("done"))
///     }
/// }
/// ```
///
/// becomes in python:
/// ```py
/// from things import do_thing, do_thing_async
/// assert do_thing() == "done"
/// assert await do_thing_async() == "done"
/// ```
macro_rules! py_function_sync_async {
    (
        $(#[$meta: meta])+
        async fn $name: ident($($arg: ident : $kind: ty),* $(,)?) $(-> $ret: ty)? $body: block
    ) => {
        async fn $name($($arg: $kind,)*) $(-> $ret)? {
            $body
        }

        ::paste::paste! {
        $(#[$meta])+
        #[pyo3(name = $name "")]
        pub fn [< py_ $name >](py: ::pyo3::Python<'_> $(, $arg: $kind)*) $(-> $ret)? {
            $crate::py_sync::py_sync!(py, $name($($arg),*))
        }

        $(#[$meta])+
        #[pyo3(name = $name "_async")]
        pub fn [< py_ $name _async >](py: ::pyo3::Python<'_> $(, $arg: $kind)*) -> ::pyo3::PyResult<&::pyo3::PyAny> {
            $crate::py_sync::py_async!(py, $name($($arg),*))
        }
        }
    };
}

pub(crate) use py_async;
pub(crate) use py_function_sync_async;
pub(crate) use py_sync;
