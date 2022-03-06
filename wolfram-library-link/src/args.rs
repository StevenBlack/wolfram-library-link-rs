//! Traits for working with data types that can be passed natively via LibraryLink
//! [`MArgument`]s.

use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    os::raw::c_char,
};

use ref_cast::RefCast;

use crate::{
    expr::{Expr, ExprKind, Symbol},
    rtl,
    sys::{self, mint, mreal, MArgument},
    wstp::Link,
    DataStore, Image, NumericArray,
};

/// Trait implemented for types that can be passed via an [`MArgument`].
pub trait FromArg<'a> {
    #[allow(missing_docs)]
    unsafe fn from_arg(arg: &'a MArgument) -> Self;

    /// Return the *LibraryLink* parameter type as a Wolfram Language expression.
    ///
    /// ```
    /// use wolfram_library_link::{FromArg, NumericArray};
    ///
    /// assert_eq!(&bool::parameter_type().to_string(), "\"Boolean\"");
    /// assert_eq!(&i64::parameter_type().to_string(), "System`Integer");
    /// assert_eq!(
    ///     &<&NumericArray<i8>>::parameter_type().to_string(),
    ///     r#"System`List[System`LibraryDataType[System`NumericArray, "Integer8"], "Constant"]"#
    /// );
    /// ```
    ///
    /// See also [`IntoArg::return_type()`] and [`NativeFunction::signature()`].
    fn parameter_type() -> Expr;
}

/// Trait implemented for types that can be returned via an [`MArgument`].
///
/// The [`MArgument`] that this trait is used to modify must be the return value of a
/// LibraryLink function. It is not valid to modify [`MArgument`]s that contain
/// LibraryLink function arguments.
pub trait IntoArg {
    /// Move `self` into `arg`.
    ///
    /// # Safety
    ///
    /// `arg` must be an uninitialized [`MArgument`] that is used to store the return
    /// value of a LibraryLink function. The return type of that function must match
    /// the type of `self.`
    ///
    /// This function must only be called immediately before returning from a LibraryLink
    /// function. Each native LibraryLink function must perform at most one call to this
    /// method per invocation.
    //
    // Private implementation note:
    //   the "at most one call to this method per invocation" constraint is necessary to
    //   maintain the safety invariants of `impl IntoArg for CString`.
    unsafe fn into_arg(self, arg: MArgument);

    /// Return the *LibraryLink* return type as a Wolfram Language expression.
    ///
    /// See also [`FromArg::parameter_type()`] and [`NativeFunction::signature()`].
    fn return_type() -> Expr;
}

/// Trait implemented for any function whose parameters and return type are native
/// LibraryLink [`MArgument`] types.
///
/// [`#[export]`][crate::export] can only be used with functions that implement this trait.
///
/// A function implements this trait if all of its parameters implement [`FromArg`] and
/// its return type implements [`IntoArg`].
///
/// Functions that pass their arguments and return value using a [`wstp::Link`] do not
/// implement this trait. See [`WstpFunction`].
pub trait NativeFunction<'a> {
    /// Call the function using the raw LibraryLink [`MArgument`] fields.
    unsafe fn call(&self, args: &'a [MArgument], ret: MArgument);

    /// Get the type signature of this function, suitable for use in
    /// [`LibraryFunctionLoad`][ref/LibraryFunctionLoad]<code>[_, _, <i>parameters</i>, <i>ret</i>]</code>.
    ///
    /// [ref/LibraryFunctionLoad]: https://reference.wolfram.com/language/ref/LibraryFunctionLoad.html
    ///
    /// See also [`FromArg::parameter_type()`] and [`IntoArg::return_type()`].
    ///
    /// The function generated by [`generate_loader!`] uses this method to generate the
    /// type signature for functions exported by [`#[export]`][crate::export].
    // Note: This method takes `self` so that it is object safe.
    fn signature(&self) -> Result<(Vec<Expr>, Expr), String>;
}

/// Trait implemented for any function whose parameters and return type can be passed
/// over a WSTP [`Link`][crate::wstp::Link].
///
/// [`#[export(wstp)]`][crate::export#exportwstp] can only be used with functions that implement
/// this trait.
///
/// A function implements this trait if its type signature is one of:
///
/// * `fn(_: &mut Link)`
/// * `fn(_: Vec<Expr>) -> Expr`
/// * `fn(_: Vec<Expr>)`
pub trait WstpFunction {
    /// Call the function using the [`Link`] object passed by the Kernel.
    unsafe fn call(&self, link: &mut Link);
}

//======================================
// FromArg Impls
//======================================

impl FromArg<'_> for bool {
    unsafe fn from_arg(arg: &MArgument) -> Self {
        crate::bool_from_mbool(*arg.boolean)
    }

    fn parameter_type() -> Expr {
        Expr::string("Boolean")
    }
}

impl FromArg<'_> for mint {
    unsafe fn from_arg(arg: &MArgument) -> Self {
        *arg.integer
    }

    fn parameter_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

impl FromArg<'_> for mreal {
    unsafe fn from_arg(arg: &MArgument) -> Self {
        *arg.real
    }

    fn parameter_type() -> Expr {
        Expr::symbol(Symbol::new("System`Real"))
    }
}

impl FromArg<'_> for sys::mcomplex {
    unsafe fn from_arg(arg: &MArgument) -> Self {
        *arg.cmplex
    }

    fn parameter_type() -> Expr {
        Expr::symbol(Symbol::new("System`Complex"))
    }
}

//--------------------------------------
// Strings
//--------------------------------------

unsafe fn c_str_from_arg<'a>(arg: &'a MArgument) -> &'a CStr {
    let cstr: *mut i8 = *arg.utf8string;
    CStr::from_ptr(cstr)
}

impl<'a> FromArg<'a> for CString {
    unsafe fn from_arg(arg: &'a MArgument) -> CString {
        let owned = {
            let cstr: &'a CStr = c_str_from_arg(arg);
            CString::from(cstr)
        };

        // Now that we own our own copy of the string, disown the Kernel's copy.
        rtl::UTF8String_disown(*arg.utf8string);

        owned
    }

    fn parameter_type() -> Expr {
        Expr::symbol(Symbol::new("System`String"))
    }
}

/// # Panics
///
/// This conversion will panic if the [`MArgument::utf8string`] field is not valid UTF-8.
impl<'a> FromArg<'a> for String {
    unsafe fn from_arg(arg: &'a MArgument) -> String {
        let owned = {
            let cstr: &'a CStr = c_str_from_arg(arg);
            let str: &'a str = cstr
                .to_str()
                .expect("FromArg for &str: string was not valid UTF-8");
            str.to_owned()
        };

        // Now that we own our own copy of the string, disown the Kernel's copy.
        rtl::UTF8String_disown(*arg.utf8string);

        owned
    }

    fn parameter_type() -> Expr {
        Expr::symbol(Symbol::new("System`String"))
    }
}

// TODO: Supported borrowed &CStr and &str's using some kind of wrapper that ensures we
//       disown the Kernel string.

/// # Safety
///
/// The lifetime of the returned `&CStr` must be the same as the lifetime of `arg`.
///
/// # Warning
///
/// Using `&CStr` as the parameter type of a *LibraryLink* function will result in a
/// memory leak. Use [`String`] or [`CString`] instead.
impl<'a> FromArg<'a> for &'a CStr {
    unsafe fn from_arg(arg: &'a MArgument) -> &'a CStr {
        c_str_from_arg(arg)
    }

    fn parameter_type() -> Expr {
        // This type implements `FromArg` purely for usage in DataStoreNode::value()
        // (via `FromArg for &str`).
        panic!("&CStr cannot be used as a LibraryLink function parameter type")
    }
}

/// # Panics
///
/// This conversion will panic if the [`MArgument::utf8string`] field is not valid UTF-8.
///
/// # Safety
///
/// The lifetime of the returned `&str` must be the same as the lifetime of `arg`.
///
/// # Warning
///
/// Using `&str` as the parameter type of a *LibraryLink* function will result in a
/// memory leak. Use [`String`] or [`CString`] instead.
impl<'a> FromArg<'a> for &'a str {
    unsafe fn from_arg(arg: &'a MArgument) -> &'a str {
        let cstr: &'a CStr = FromArg::<'a>::from_arg(arg);
        cstr.to_str()
            .expect("FromArg for &str: string was not valid UTF-8")
    }

    fn parameter_type() -> Expr {
        // This type implements `FromArg` purely for usage in DataStoreNode::value().
        panic!("&str cannot be used as a LibraryLink function parameter type")
    }
}

//--------------------------------------
// NumericArray
//--------------------------------------

// TODO: Add FromArg for NumericArray which just clones the numeric array? Or disclaims
//       ownership in another way?


/// # Safety
///
/// `FromArg for NumericArray<T>` MUST be constrained by `T: NumericArrayType` to prevent
/// accidental creation of invalid `NumericArray` conversions. Without this constraint,
/// it would be possible to write code like:
///
/// ```compile_fail
/// # mod scope {
/// # use wolfram_library_link::{export, NumericArray};
/// #[export] // Unsafe!
/// fn and(bools: NumericArray<bool>) -> bool {
///     // ...
/// #   todo!()
/// }
/// # }
/// ```
///
/// which is not valid because `bool` is not a valid numeric array type.
impl<'a, T: crate::NumericArrayType> FromArg<'a> for &'a NumericArray<T> {
    unsafe fn from_arg(arg: &'a MArgument) -> &'a NumericArray<T> {
        NumericArray::ref_cast(&*arg.numeric)
    }

    fn parameter_type() -> Expr {
        // NOTE:
        //   We use "Constant" instead of Automatic as the default memory management
        //   strategy for &NumericArray<T> (and &Image<T> as well). This is because, for
        //   both Automatic and "Constant", the fact remains that on the Rust side we have
        //   an immutable reference to a NumericArray -- we're not going to free the array,
        //   and we're not going to mutate. Using Automatic would behave correctly, but
        //   would incur an unnecessary deep clone of the array contents as Automatic
        //   doesn't imply any explicit promise from the programmer that they will not
        //   mutate the array (unlike "Constant", which *is* an explicit promise that we
        //   won't mutate the array the Kernel passes in).

        // {LibraryDataType[NumericArray, "<T>"], "Constant"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::normal(Symbol::new("System`LibraryDataType"), vec![
                Expr::from(Symbol::new("System`NumericArray")),
                Expr::string(T::TYPE.name()),
            ]),
            Expr::string("Constant"),
        ])
    }
}

impl<'a, T: crate::NumericArrayType> FromArg<'a> for NumericArray<T> {
    unsafe fn from_arg(arg: &'a MArgument) -> NumericArray<T> {
        NumericArray::from_raw(*arg.numeric)
    }

    fn parameter_type() -> Expr {
        // {LibraryDataType[NumericArray, "<T>"], "Shared"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::normal(Symbol::new("System`LibraryDataType"), vec![
                Expr::from(Symbol::new("System`NumericArray")),
                Expr::string(T::TYPE.name()),
            ]),
            Expr::string("Shared"),
        ])
    }
}

impl<'a> FromArg<'a> for &'a NumericArray<()> {
    unsafe fn from_arg(arg: &'a MArgument) -> &'a NumericArray<()> {
        NumericArray::ref_cast(&*arg.numeric)
    }

    fn parameter_type() -> Expr {
        // {NumericArray, "Constant"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::from(Symbol::new("System`NumericArray")),
            Expr::string("Constant"),
        ])
    }
}

impl<'a> FromArg<'a> for NumericArray<()> {
    unsafe fn from_arg(arg: &'a MArgument) -> NumericArray<()> {
        NumericArray::from_raw(*arg.numeric)
    }

    fn parameter_type() -> Expr {
        // {NumericArray, "Shared"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::from(Symbol::new("System`NumericArray")),
            Expr::string("Shared"),
        ])
    }
}

//--------------------------------------
// Image
//--------------------------------------

impl<'a, T: crate::ImageData> FromArg<'a> for &'a Image<T> {
    unsafe fn from_arg(arg: &'a MArgument) -> &'a Image<T> {
        Image::ref_cast(&*arg.image)
    }

    fn parameter_type() -> Expr {
        // {LibraryDataType[Image | Image3D, "<T>"], "Constant"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::normal(Symbol::new("System`LibraryDataType"), vec![
                Expr::normal(Symbol::new("System`Alternatives"), vec![
                    Expr::from(Symbol::new("System`Image")),
                    Expr::from(Symbol::new("System`Image3D")),
                ]),
                Expr::string(T::TYPE.name()),
            ]),
            Expr::string("Constant"),
        ])
    }
}

impl<'a, T: crate::ImageData> FromArg<'a> for Image<T> {
    unsafe fn from_arg(arg: &'a MArgument) -> Image<T> {
        Image::from_raw(*arg.image)
    }

    fn parameter_type() -> Expr {
        // {LibraryDataType[Image | Image3D, "<T>"], "Shared"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::normal(Symbol::new("System`LibraryDataType"), vec![
                Expr::normal(Symbol::new("System`Alternatives"), vec![
                    Expr::from(Symbol::new("System`Image")),
                    Expr::from(Symbol::new("System`Image3D")),
                ]),
                Expr::string(T::TYPE.name()),
            ]),
            Expr::string("Shared"),
        ])
    }
}

impl<'a> FromArg<'a> for &'a Image<()> {
    unsafe fn from_arg(arg: &'a MArgument) -> &'a Image<()> {
        Image::ref_cast(&*arg.image)
    }

    fn parameter_type() -> Expr {
        // {Image | Image3D, "Constant"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::normal(Symbol::new("System`Alternatives"), vec![
                Expr::from(Symbol::new("System`Image")),
                Expr::from(Symbol::new("System`Image3D")),
            ]),
            Expr::string("Constant"),
        ])
    }
}

impl<'a> FromArg<'a> for Image<()> {
    unsafe fn from_arg(arg: &'a MArgument) -> Image<()> {
        Image::from_raw(*arg.image)
    }

    fn parameter_type() -> Expr {
        // {Image | Image3D, "Shared"}
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::normal(Symbol::new("System`Alternatives"), vec![
                Expr::from(Symbol::new("System`Image")),
                Expr::from(Symbol::new("System`Image3D")),
            ]),
            Expr::string("Shared"),
        ])
    }
}

//--------------------------------------
// DataStore
//--------------------------------------

impl FromArg<'_> for DataStore {
    unsafe fn from_arg(arg: &MArgument) -> DataStore {
        DataStore::from_raw(*arg.tensor as sys::DataStore)
    }

    fn parameter_type() -> Expr {
        Expr::string("DataStore")
    }
}

impl<'a> FromArg<'a> for &'a DataStore {
    unsafe fn from_arg(arg: &MArgument) -> &'a DataStore {
        DataStore::ref_cast(&*(arg.tensor as *mut sys::DataStore))
    }

    fn parameter_type() -> Expr {
        // This type implements `FromArg` purely for usage in DataStoreNode::value().
        panic!("&DataStore cannot be used as a LibraryLink function parameter type")
    }
}

//======================================
// impl IntoArg
//======================================

impl IntoArg for () {
    unsafe fn into_arg(self, _arg: MArgument) {
        // Do nothing.
    }

    fn return_type() -> Expr {
        Expr::string("Void")
    }
}

//---------------------
// Primitive data types
//---------------------

impl IntoArg for bool {
    unsafe fn into_arg(self, arg: MArgument) {
        let boole: u32 = if self { sys::True } else { sys::False };
        *arg.boolean = boole as sys::mbool;
    }

    fn return_type() -> Expr {
        Expr::string("Boolean")
    }
}

impl IntoArg for mint {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.integer = self;
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

impl IntoArg for mreal {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.real = self;
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Real"))
    }
}

impl IntoArg for sys::mcomplex {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.cmplex = self;
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Complex"))
    }
}

//--------------------------------------------------
// Convenience conversions for narrow integer sizes.
//--------------------------------------------------

impl IntoArg for i8 {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.integer = mint::from(self);
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

impl IntoArg for i16 {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.integer = mint::from(self);
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

impl IntoArg for i32 {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.integer = mint::from(self);
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

impl IntoArg for u8 {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.integer = mint::from(self);
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

impl IntoArg for u16 {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.integer = mint::from(self);
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

// If we're on a 32 bit platform, mint might be an alias for i32. Avoid providing this
// conversion on those platforms.
#[cfg(target_pointer_width = "64")]
impl IntoArg for u32 {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.integer = mint::from(self);
    }

    fn return_type() -> Expr {
        Expr::symbol(Symbol::new("System`Integer"))
    }
}

//--------------------
// Strings
//--------------------

thread_local! {
    /// See [`<CString as IntoArg>::into_arg()`] for information about how this static is
    /// used.
    static RETURNED_STRING: RefCell<Option<CString>> = RefCell::new(None);
}

impl IntoArg for CString {
    unsafe fn into_arg(self, arg: MArgument) {
        // Extend the lifetime of `self.as_ptr()` by storing `self` in `RETURNED_STRING`.
        //
        // This will keep `raw` valid past the point that the current LibraryLink
        // function returns, at which point it will be copied by the Kernel and is no
        // longer used. We'll drop `self` the next time this function is called.
        //
        // This implementation limits the maximum number of "leaked" strings to just one.
        //
        // For more information on management of string memory when passed via
        // LibraryLink, see:
        //
        // <https://reference.wolfram.com/language/LibraryLink/tutorial/InteractionWithWolframLanguage.html#262826222>
        let raw: *const c_char = RETURNED_STRING.with(|stored| {
            // Drop the previously returned string, if any.
            if let Some(prev) = stored.replace(None) {
                drop(prev);
            }

            let raw: *const c_char = self.as_ptr();

            *stored.borrow_mut() = Some(self);

            raw
        });

        // Return `raw` via this MArgument.
        *arg.utf8string = raw as *mut c_char;
    }

    fn return_type() -> Expr {
        Expr::from(Symbol::new("System`String"))
    }
}

impl IntoArg for String {
    /// # Panics
    ///
    /// This function will panic if `self` cannot be converted into a [`CString`].
    unsafe fn into_arg(self, arg: MArgument) {
        let cstring = CString::new(self)
            .expect("IntoArg for String: could not convert String to CString");

        <CString as IntoArg>::into_arg(cstring, arg)
    }

    fn return_type() -> Expr {
        Expr::from(Symbol::new("System`String"))
    }
}

//---------------------------------------
// NumericArray, Image, DataStore
//---------------------------------------

impl<T: crate::NumericArrayType> IntoArg for NumericArray<T> {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.numeric = self.into_raw();
    }

    fn return_type() -> Expr {
        // LibraryDataType[NumericArray, "<T>"]
        Expr::normal(Symbol::new("System`LibraryDataType"), vec![
            Expr::from(Symbol::new("System`NumericArray")),
            Expr::string(T::TYPE.name()),
        ])
    }
}

impl IntoArg for NumericArray<()> {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.numeric = self.into_raw();
    }

    fn return_type() -> Expr {
        // NumericArray
        Expr::from(Symbol::new("System`NumericArray"))
    }
}

impl<T: crate::ImageData> IntoArg for Image<T> {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.image = self.into_raw();
    }

    fn return_type() -> Expr {
        // LibraryDataType[Image | Image3D, "<T>"]
        Expr::normal(Symbol::new("System`List"), vec![
            Expr::normal(Symbol::new("System`LibraryDataType"), vec![
                Expr::normal(Symbol::new("System`Alternatives"), vec![
                    Expr::from(Symbol::new("System`Image")),
                    Expr::from(Symbol::new("System`Image3D")),
                ]),
                Expr::string(T::TYPE.name()),
            ]),
            Expr::string("Shared"),
        ])
    }
}

impl IntoArg for DataStore {
    unsafe fn into_arg(self, arg: MArgument) {
        *arg.tensor = self.into_raw() as *mut _;
    }

    fn return_type() -> Expr {
        Expr::string("DataStore")
    }
}

//======================================
// impl NativeFunction
//======================================

/// Implement `NativeFunction` for functions that use raw [`MArgument`]s for their
/// arguments and return value.
///
/// # Example
///
/// ```
/// # mod scope {
/// use wolfram_library_link::{self as wll, sys::MArgument, FromArg};
///
/// #[wll::export]
/// fn raw_add2(args: &[MArgument], ret: MArgument) {
///     let x = unsafe { i64::from_arg(&args[0]) };
///     let y = unsafe { i64::from_arg(&args[1]) };
///
///     unsafe {
///         *ret.integer = x + y;
///     }
/// }
/// # }
/// ```
///
/// ```wolfram
/// LibraryFunctionLoad["...", "raw_add2", {Integer, Integer}, Integer]
/// ```
impl<'a: 'b, 'b> NativeFunction<'a> for fn(&'b [MArgument], MArgument) {
    unsafe fn call(&self, args: &'a [MArgument], ret: MArgument) {
        self(args, ret)
    }

    fn signature(&self) -> Result<(Vec<Expr>, Expr), String> {
        Err(
            "fn(&[MArgument], MArgument) function cannot be loaded automatically: \
            parameter and return types are unknown."
                .to_owned(),
        )
    }
}

//--------------------
// impl NativeFunction
//--------------------

macro_rules! impl_NativeFunction {
    ($($type:ident),*) => {
        impl<'a, $($type,)* R> NativeFunction<'a> for fn($($type),*) -> R
        where
            R: IntoArg,
            $($type: FromArg<'a>),*
        {
            unsafe fn call(&self, args: &'a [MArgument], ret: MArgument) {
                // Re-use the $type name as the local variable names. E.g.
                //     let A1 = A1::from_arg(..);
                // This works because types and variable names are different namespaces.
                #[allow(non_snake_case)]
                let [$($type,)*] = match args {
                    [$($type,)*] => [$($type,)*],
                    _ => panic!(
                        "LibraryLink function number of arguments ({}) does not match \
                        number of parameters",
                        args.len()
                    ),
                };

                $(
                    #[allow(non_snake_case)]
                    let $type: $type = $type::from_arg($type);
                )*

                let result: R = self($($type,)*);

                result.into_arg(ret);
            }

            fn signature(&self) -> Result<(Vec<Expr>, Expr), String> {
                let mut param_tys = Vec::new();

                $(
                    param_tys.push($type::parameter_type());
                )*

                Ok((param_tys, R::return_type()))
            }
        }
    }
}

// Handle the zero-arguments case specially.
impl<'a, R> NativeFunction<'a> for fn() -> R
where
    R: IntoArg,
{
    unsafe fn call(&self, args: &[MArgument], ret: MArgument) {
        if args.len() != 0 {
            panic!(
                "LibraryLink function number of arguments ({}) does not match number of \
                parameters",
                args.len()
            );
        }

        let result = self();

        result.into_arg(ret);
    }

    fn signature(&self) -> Result<(Vec<Expr>, Expr), String> {
        Ok((Vec::new(), R::return_type()))
    }
}

impl_NativeFunction!(A1);
impl_NativeFunction!(A1, A2);
impl_NativeFunction!(A1, A2, A3);
impl_NativeFunction!(A1, A2, A3, A4);
impl_NativeFunction!(A1, A2, A3, A4, A5);
impl_NativeFunction!(A1, A2, A3, A4, A5, A6);
impl_NativeFunction!(A1, A2, A3, A4, A5, A6, A7);
impl_NativeFunction!(A1, A2, A3, A4, A5, A6, A7, A8);
impl_NativeFunction!(A1, A2, A3, A4, A5, A6, A7, A8, A9);
impl_NativeFunction!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10);
impl_NativeFunction!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11);
impl_NativeFunction!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12);

//======================================
// impl WstpFunction
//======================================

/// Implement [`WstpFunction`] for functions that use a [`Link`] for their arguments and
/// return value.
///
/// # Example
///
/// ```
/// # mod scope {
/// use wolfram_library_link::{self as wll, wstp::Link};
///
/// #[wll::export(wstp)]
/// fn add2_link(link: &mut Link) {
///     let argc: usize = link.test_head("List").unwrap();
///
///     if argc != 2 {
///         panic!("expected 2 arguments, got {}", argc);
///     }
///
///     let x = link.get_i64().unwrap();
///     let y = link.get_i64().unwrap();
///
///     link.put_i64(x + y).unwrap();
/// }
/// # }
/// ```
///
/// ```wolfram
/// LibraryFunctionLoad["...", "add2_link", LinkObject, LinkObject]
/// ```
impl WstpFunction for fn(&mut Link) {
    unsafe fn call(&self, link: &mut Link) {
        self(link)
    }
}

/// Implement [`WstpFunction`] for functions that use [`Expr`] for their arguments and
/// return value.
///
/// # Example
///
/// ```
/// # mod scope {
/// use wolfram_library_link::{self as wll, wstp::Link, expr::{Expr, ExprKind}};
///
/// #[wll::export(wstp)]
/// fn add2(args: Vec<Expr>) -> Expr {
///     if args.len() != 2 {
///         panic!("expected 2 arguments, got {}", args.len());
///     }
///
///     let x: i64 = match *args[0].kind() {
///         ExprKind::Integer(value) => value,
///         _ => panic!("expected 1st argument to be Integer, got: {}", args[0])
///     };
///     let y: i64 = match *args[1].kind() {
///         ExprKind::Integer(value) => value,
///         _ => panic!("expected 2nd argument to be Integer, got: {}", args[1])
///     };
///
///     Expr::from(x + y)
/// }
/// # }
/// ```
///
/// ```wolfram
/// LibraryFunctionLoad["...", "add2", LinkObject, LinkObject]
/// ```
impl WstpFunction for fn(Vec<Expr>) -> Expr {
    unsafe fn call(&self, link: &mut Link) {
        let args: Vec<Expr> = match get_args_list(link) {
            Ok(args) => args,
            Err(message) => panic!("WstpFunction: {}", message),
        };

        let result: Expr = self(args);

        match link.put_expr(&result) {
            Ok(()) => (),
            Err(err) => panic!(
                "WstpFunction: WSTP error writing return expression to link: {}",
                err
            ),
        }
    }
}

impl WstpFunction for fn(Vec<Expr>) {
    unsafe fn call(&self, link: &mut Link) {
        let args: Vec<Expr> = match get_args_list(link) {
            Ok(args) => args,
            Err(message) => panic!("WstpFunction: {}", message),
        };

        let _null: () = self(args);

        match link.put_symbol("System`Null") {
            Ok(()) => (),
            Err(err) => panic!(
                "WstpFunction: WSTP error writing return Null expression to link: {}",
                err
            ),
        }
    }
}

//----------------------------
// Utilities
//----------------------------

fn get_args_list(link: &mut Link) -> Result<Vec<Expr>, String> {
    let list = match link.get_expr() {
        Ok(args) => args,
        Err(err) => {
            return Err(format!(
                "WSTP error reading argument List expression: {}",
                err
            ))
        },
    };

    let list = match list.to_kind() {
        ExprKind::Normal(list) => list,
        _ => return Err("expected List expression".to_owned()),
    };

    if !list.has_head(&Symbol::new("System`List")) {
        return Err("expected List expression".to_owned());
    }

    Ok(list.into_elements())
}
