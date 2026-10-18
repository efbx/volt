//! The `#[coerce]` attribute for `volt-coerce`: lenient JSON reading on top of serde.
//!
//! [`coerce`] turns a struct or enum into a serde `Deserialize` implementation that reads
//! through `volt_coerce::Coerce`, and [`SerdeAttrs`] keeps `#[serde]` attributes legal on the
//! annotated struct. Use both through `volt-coerce`, which re-exports them: the generated code
//! refers to `::volt_coerce`.

#![warn(missing_docs)]

use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;
use syn::{
    Attribute, Expr, ExprLit, ExprPath, Field, Fields, Item, ItemEnum, ItemStruct, Lit, LitStr,
    Meta, Path, Token, Type, Visibility, parse_macro_input, parse_quote,
};

/// The arguments of `#[coerce(…)]`.
struct Options {
    /// Whether `custom` was given: the type implements `Coerce` by hand.
    custom: bool,
    /// Whether `take_from` was given: the struct also gets a `take_from` constructor.
    take_from: bool,
    /// The hook named by `after = path`, run on the decoded value.
    after: Option<ExprPath>,
}

impl Options {
    /// Parses `args`, accepting only `custom`, `take_from` and `after = path`; anything else is a
    /// compile error.
    fn parse(args: proc_macro::TokenStream) -> syn::Result<Self> {
        let mut options = Self {
            custom: false,
            take_from: false,
            after: None,
        };
        let parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("custom") {
                options.custom = true;
                Ok(())
            } else if meta.path.is_ident("take_from") {
                options.take_from = true;
                Ok(())
            } else if meta.path.is_ident("after") {
                options.after = Some(meta.value()?.parse()?);
                Ok(())
            } else {
                Err(meta.error("expected `custom`, `take_from` or `after = path`"))
            }
        });
        Parser::parse(parser, args)?;
        Ok(options)
    }
}

/// The entries of every `#[serde(…)]` attribute on one struct, enum or field, in one list.
struct Serde {
    metas: Vec<Meta>,
}

impl Serde {
    /// Collects the entries of the `#[serde]` attributes among `attrs`, ignoring other attributes.
    fn read(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut metas = Vec::new();
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
            metas.extend(attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?);
        }
        Ok(Self { metas })
    }

    /// Whether an entry named `name` is present, with or without a value.
    fn has(&self, name: &str) -> bool {
        self.metas.iter().any(|meta| meta.path().is_ident(name))
    }

    /// Returns the string literal of the first `name = "…"` entry, if any.
    fn string(&self, name: &str) -> Option<&LitStr> {
        self.metas.iter().find_map(|meta| match meta {
            Meta::NameValue(pair) if pair.path.is_ident(name) => match &pair.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(text),
                    ..
                }) => Some(text),
                _ => None,
            },
            _ => None,
        })
    }

    /// Whether serde decodes the field itself (`flatten`, `skip`, `skip_deserializing`, `with` or
    /// `deserialize_with`), so it gets no lenient reader.
    fn owned(&self) -> bool {
        [
            "flatten",
            "skip",
            "skip_deserializing",
            "with",
            "deserialize_with",
        ]
        .iter()
        .any(|name| self.has(name))
    }

    /// Returns the type named by `from` or `try_from`, with `true` for the fallible `try_from`;
    /// `from` wins when both are present.
    fn twin(&self) -> Option<(&LitStr, bool)> {
        match (self.string("from"), self.string("try_from")) {
            (Some(raw), _) => Some((raw, false)),
            (None, Some(raw)) => Some((raw, true)),
            (None, None) => None,
        }
    }

    /// Returns the container entries to copy onto the proxy: every one except `into`, with a bare
    /// `default` dropped when `derived` and otherwise rewritten to name `owner`'s `Default`
    /// explicitly.
    fn proxy_metas(&self, owner: &Ident, derived: bool) -> impl Iterator<Item = TokenStream> {
        self.metas.iter().filter_map(move |meta| match meta {
            Meta::Path(path) if path.is_ident("default") => (!derived).then(|| {
                let default = LitStr::new(
                    &format!("<{owner} as ::core::default::Default>::default"),
                    owner.span(),
                );
                quote!(default = #default)
            }),
            other if other.path().is_ident("into") => None,
            other => Some(quote!(#other)),
        })
    }
}

/// Replaces a leading `Self` in field types with the owner's name, so the types still resolve
/// inside the generated proxy struct.
struct Receiver(Ident);

impl VisitMut for Receiver {
    fn visit_path_mut(&mut self, path: &mut Path) {
        if path.segments[0].ident == "Self" {
            path.segments[0].ident = self.0.clone();
        }
        syn::visit_mut::visit_path_mut(self, path);
    }
}

/// The field markers `#[strict]` and `#[reject]`, which `#[coerce]` consumes.
struct Mark;

impl Mark {
    /// Removes every `#[name]` attribute from `attrs` and returns whether there was one.
    fn take(attrs: &mut Vec<Attribute>, name: &str) -> bool {
        let found = attrs.iter().any(|attr| attr.path().is_ident(name));
        attrs.retain(|attr| !attr.path().is_ident(name));
        found
    }
}

/// What a field without a default of its own falls back to, as set by the struct's own
/// `#[serde(default)]`.
enum ContainerDefault {
    /// No container default: only an `Option` field falls back, to `None`.
    Absent,
    /// A bare `#[serde(default)]` that stands for std's derived `Default`: each field takes its
    /// type's default directly and no whole struct is built.
    Derived,
    /// Any other container default: the field of the whole value this expression builds.
    Built(TokenStream),
}

impl ContainerDefault {
    /// Reads the container default of `item` from its `#[serde]` entries in `container`: a bare
    /// `default` is [`ContainerDefault::Derived`] when `item` derives `Default`, gives no field a
    /// `= value` default and is neither `transparent` nor deprecated; any other is
    /// [`ContainerDefault::Built`].
    fn read(item: &ItemStruct, container: &Serde) -> syn::Result<Self> {
        let owner = &item.ident;
        Ok(match container.string("default") {
            Some(path) => {
                let path: ExprPath = path.parse()?;
                Self::Built(quote!(#path()))
            }
            None if !container.has("default") => Self::Absent,
            None if !container.has("transparent")
                && Self::derives_default(&item.attrs)
                && item.fields.iter().all(|field| field.default.is_none())
                && !Self::deprecated(item) =>
            {
                Self::Derived
            }
            None => Self::Built(quote!(<#owner as ::core::default::Default>::default())),
        })
    }

    /// Whether a `#[derive(…)]` among `attrs` lists an unqualified `Default`, taken to be std's
    /// derive.
    fn derives_default(attrs: &[Attribute]) -> bool {
        attrs
            .iter()
            .filter(|attr| Self::named(attr.path(), "derive"))
            .filter_map(|attr| {
                attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)
                    .ok()
            })
            .any(|paths| paths.iter().any(|path| Self::named(path, "Default")))
    }

    /// Whether the struct or any of its fields is `#[deprecated]`, directly or inside a
    /// `cfg_attr`, in which case a read written by `#[coerce]` would warn where serde's does not.
    fn deprecated(item: &ItemStruct) -> bool {
        item.attrs
            .iter()
            .chain(item.fields.iter().flat_map(|field| &field.attrs))
            .any(|attr| match &attr.meta {
                Meta::List(list) if Self::named(&list.path, "cfg_attr") => {
                    Self::mentions_deprecated(list.tokens.clone())
                }
                meta => Self::named(meta.path(), "deprecated"),
            })
    }

    /// Whether `tokens` hold the identifier `deprecated`, raw or not, at any depth.
    fn mentions_deprecated(tokens: TokenStream) -> bool {
        tokens.into_iter().any(|token| match token {
            TokenTree::Ident(ident) => ident.unraw() == "deprecated",
            TokenTree::Group(group) => Self::mentions_deprecated(group.stream()),
            _ => false,
        })
    }

    /// Whether `path` is the single identifier `name`, raw (`r#name`) or not.
    fn named(path: &Path, name: &str) -> bool {
        path.get_ident().is_some_and(|ident| ident.unraw() == name)
    }
}

/// The expansion of `#[coerce]` on a struct: a serde remote proxy whose fields read through
/// `volt_coerce::Reader`, plus the owner's `Deserialize` and `Coerce` implementations.
struct Lenient {
    /// The name of the annotated struct.
    owner: Ident,
    /// The name of the generated proxy, `__{owner}Coerce`.
    proxy: Ident,
    /// The struct's own `#[serde]` entries.
    container: Serde,
    /// What a field without a default of its own falls back to.
    default: ContainerDefault,
    /// The `after` hook, if any.
    after: Option<ExprPath>,
    /// Whether to generate `take_from`.
    take_from: bool,
}

impl Lenient {
    /// Expands `item`. A struct with `#[serde(from)]` or `#[serde(try_from)]` derives serde as is
    /// and reads through the source type, dropping `#[strict]`, `#[reject]` and the `after` hook;
    /// any other struct goes through the proxy and needs named fields. `take_from` is refused on
    /// the former.
    fn expand(
        mut item: ItemStruct,
        after: Option<ExprPath>,
        take_from: bool,
    ) -> syn::Result<TokenStream> {
        let owner = item.ident.clone();
        for field in &mut item.fields {
            Receiver(owner.clone()).visit_type_mut(&mut field.ty);
        }
        let container = Serde::read(&item.attrs)?;
        if let Some((raw, fallible)) = container.twin() {
            if take_from {
                return Err(syn::Error::new_spanned(
                    &owner,
                    "`take_from` needs a struct without `#[serde(from)]` or `#[serde(try_from)]`",
                ));
            }
            for field in &mut item.fields {
                Mark::take(&mut field.attrs, "strict");
                Mark::take(&mut field.attrs, "reject");
            }
            let rule = Twin::rule(&owner, &raw.parse()?, fallible);
            let item = Self::deserialized(item);
            return Ok(quote! {
                #item

                const _: () = {
                    #rule
                };
            });
        }
        let Fields::Named(_) = &item.fields else {
            return Err(syn::Error::new_spanned(
                &item.ident,
                "#[coerce] reads named fields",
            ));
        };
        Self {
            proxy: format_ident!("__{owner}Coerce"),
            default: ContainerDefault::read(&item, &container)?,
            container,
            owner,
            after,
            take_from,
        }
        .proxy(item)
    }

    /// Returns `item` with `#[derive(::serde::Deserialize)]` prepended.
    fn deserialized(mut item: ItemStruct) -> ItemStruct {
        item.attrs
            .insert(0, parse_quote!(#[derive(::serde::Deserialize)]));
        item
    }

    /// Generates the proxy and the owner's `Deserialize` and `Coerce` implementations, running the
    /// `after` hook on the decoded value, and `take_from` when asked. A derived container default
    /// builds no whole value to read fields from, so `Deserialize` pretends to read every field
    /// without a `default` of its own except an unskipped `flatten` one, which `take_from` pretends
    /// to read instead, so that dead-code warnings match a built default's.
    fn proxy(&self, mut item: ItemStruct) -> syn::Result<TokenStream> {
        let owner = &self.owner;
        let proxy = &self.proxy;
        let derived = matches!(self.default, ContainerDefault::Derived);
        let mut readers = Vec::new();
        let mut fields = Vec::new();
        let mut taken = Vec::new();
        let mut pretended = Vec::new();
        let mut flattened = Vec::new();
        for field in &mut item.fields {
            let strict = Mark::take(&mut field.attrs, "strict");
            let reject = Mark::take(&mut field.attrs, "reject");
            let serde = Serde::read(&field.attrs)?;
            if self.take_from {
                taken.push(self.taken(field, strict, &serde)?);
            }
            let mut copy = field.clone();
            copy.vis = Visibility::Inherited;
            copy.attrs.retain(|attr| attr.path().is_ident("serde"));
            let lenient = !strict && !serde.owned();
            if lenient {
                let (name, tokens) = self.reader(field, reject, &serde)?;
                let path = LitStr::new(&format!("{proxy}::{name}"), name.span());
                copy.attrs
                    .push(parse_quote!(#[serde(deserialize_with = #path)]));
                readers.push(tokens);
            }
            let own = serde.has("default");
            let collected =
                serde.has("flatten") && !serde.has("skip") && !serde.has("skip_deserializing");
            let defaulted = match self.default {
                ContainerDefault::Absent => lenient && Self::optional(&field.ty),
                ContainerDefault::Derived => !collected,
                ContainerDefault::Built(_) => false,
            };
            if defaulted && !own {
                copy.attrs.push(parse_quote!(#[serde(default)]));
            }
            if derived && !own {
                if !collected {
                    pretended.extend(&field.ident);
                } else if self.take_from {
                    flattened.extend(&field.ident);
                }
            }
            fields.push(copy);
        }
        let remote = LitStr::new(&owner.to_string(), owner.span());
        let metas = self.container.proxy_metas(owner, derived);
        let pretend = Self::pretend(&pretended);
        let body = if let Some(after) = &self.after {
            quote! {
                let mut value = #proxy::deserialize(d)?;
                ::volt_coerce::AfterOutcome::finish(#after(&mut value))
                    .map_err(<D::Error as ::serde::de::Error>::custom)?;
                ::core::result::Result::Ok(value)
            }
        } else {
            quote!(#proxy::deserialize(d))
        };
        item.attrs
            .insert(0, parse_quote!(#[derive(::volt_coerce::__SerdeAttrs)]));
        let rule = self.object_rule();
        let take_from = if self.take_from {
            Some(self.take_from(&item.vis, &taken, &flattened)?)
        } else {
            None
        };
        Ok(quote! {
            #item

            const _: () = {
                #[derive(::serde::Deserialize)]
                #[serde(remote = #remote #(, #metas)*)]
                struct #proxy { #(#fields,)* }

                impl #proxy { #(#readers)* }

                impl<'de> ::serde::Deserialize<'de> for #owner {
                    fn deserialize<D: ::serde::Deserializer<'de>>(
                        d: D,
                    ) -> ::core::result::Result<Self, D::Error> {
                        #pretend
                        #body
                    }
                }

                #rule

                #take_from
            };
        })
    }

    /// Returns the owner's `take_from`: it removes the keys of the owner's fields from a JSON
    /// object map and reads them as the owner's `Deserialize` would, then runs the `after` hook,
    /// leaving every other key in the map, and pretends to read the `flattened` fields. Errors on a
    /// struct with `rename_all`.
    fn take_from(
        &self,
        vis: &Visibility,
        taken: &[TokenStream],
        flattened: &[&Ident],
    ) -> syn::Result<TokenStream> {
        if self.container.has("rename_all") {
            return Err(syn::Error::new_spanned(
                &self.owner,
                "`take_from` needs a struct without `rename_all`",
            ));
        }
        let owner = &self.owner;
        let pretend = Self::pretend(flattened);
        let fallback = match &self.default {
            ContainerDefault::Built(built) => Some(quote!(let __fallback: Self = #built;)),
            ContainerDefault::Absent | ContainerDefault::Derived => None,
        };
        let (binding, after) = match &self.after {
            Some(after) => (
                quote!(let mut value),
                quote! {
                    ::volt_coerce::AfterOutcome::finish(#after(&mut value)).map_err(
                        <::volt_coerce::serde_json::Error as ::serde::de::Error>::custom,
                    )?;
                },
            ),
            None => (quote!(let value), TokenStream::new()),
        };
        Ok(quote! {
            impl #owner {
                /// Removes the keys of this struct's fields from `map` and reads them as
                /// deserializing would, leaving every other key in `map`.
                ///
                /// # Errors
                ///
                /// Fails when a required key is missing or a value is rejected, as
                /// deserializing would.
                #vis fn take_from(
                    map: &mut ::volt_coerce::serde_json::Map<
                        ::std::string::String,
                        ::volt_coerce::serde_json::Value,
                    >,
                ) -> ::core::result::Result<Self, ::volt_coerce::serde_json::Error> {
                    #pretend
                    #fallback
                    #binding = Self { #(#taken,)* };
                    #after
                    ::core::result::Result::Ok(value)
                }
            }
        })
    }

    /// Returns the `take_from` initialiser of `field`: its key is removed from the map and read
    /// as `Deserialize` reads it, and a missing key gets what `Deserialize` gives one. `flatten`
    /// and `skip` fields are not looked up and get their default. Errors on `alias`, `with`,
    /// `deserialize_with` and `rename(…)`.
    fn taken(&self, field: &Field, strict: bool, serde: &Serde) -> syn::Result<TokenStream> {
        let name = field.ident.as_ref().expect("named field");
        if ["alias", "with", "deserialize_with"]
            .iter()
            .any(|attr| serde.has(attr))
            || (serde.has("rename") && serde.string("rename").is_none())
        {
            return Err(syn::Error::new_spanned(
                name,
                "`take_from` needs a field without `alias`, `with`, `deserialize_with` or `rename(…)`",
            ));
        }
        let ty = &field.ty;
        let fallback = self.fallback(name, ty, serde, true)?;
        if serde.has("flatten") || serde.has("skip") || serde.has("skip_deserializing") {
            let fallback =
                fallback.unwrap_or_else(|| quote!(<#ty as ::core::default::Default>::default()));
            return Ok(quote!(#name: #fallback));
        }
        let key = if let Some(rename) = serde.string("rename") {
            quote!(#rename)
        } else {
            let key = LitStr::new(&name.to_string(), name.span());
            quote!(#key)
        };
        let read = if strict {
            quote!(<#ty as ::serde::Deserialize>::deserialize(value)?)
        } else {
            let proxy = &self.proxy;
            let reader = format_ident!("read_{name}");
            quote!(#proxy::#reader(value)?)
        };
        let missing = fallback.unwrap_or_else(|| {
            quote! {
                return ::core::result::Result::Err(
                    <::volt_coerce::serde_json::Error as ::serde::de::Error>::missing_field(#key),
                )
            }
        });
        Ok(quote! {
            #name: match map.remove(#key) {
                ::core::option::Option::Some(value) => #read,
                ::core::option::Option::None => #missing,
            }
        })
    }

    /// Returns what the field `name` of type `ty` falls back to when its key is missing or its
    /// value is junk, in serde's order: its own `default`, then the container's, then `None` for an
    /// `Option`; `None` when nothing does. A built container default is read from `__fallback`
    /// when `bound`, where `take_from` bound it once, and is otherwise built in place.
    fn fallback(
        &self,
        name: &Ident,
        ty: &Type,
        serde: &Serde,
        bound: bool,
    ) -> syn::Result<Option<TokenStream>> {
        if let Some(path) = serde.string("default") {
            let path: ExprPath = path.parse()?;
            return Ok(Some(quote!(#path())));
        }
        let own = serde.has("default");
        Ok(match &self.default {
            ContainerDefault::Built(built) if !own => {
                let owner = &self.owner;
                Some(if bound {
                    quote!(__fallback.#name)
                } else {
                    quote!({ let __fallback: #owner = #built; __fallback.#name })
                })
            }
            ContainerDefault::Absent if !own && !Self::optional(ty) => None,
            _ => Some(quote!(<#ty as ::core::default::Default>::default())),
        })
    }

    /// Returns the name and body of the lenient reader for `field`. Junk falls as
    /// [`Lenient::fallback`] says and is otherwise an `unsupported value` error, as it always is
    /// under `#[reject]`.
    fn reader(
        &self,
        field: &Field,
        reject: bool,
        serde: &Serde,
    ) -> syn::Result<(Ident, TokenStream)> {
        let name = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        let reader = format_ident!("read_{name}");
        let key = LitStr::new(&name.to_string(), name.span());
        let fallback = if reject {
            None
        } else {
            self.fallback(name, ty, serde, false)?
        };
        let skipped = if let Some(fallback) = fallback {
            quote!(::core::result::Result::Ok(#fallback))
        } else {
            quote! {
                ::core::result::Result::Err(<D::Error as ::serde::de::Error>::custom(
                    ::core::format_args!("unsupported value for `{}`", #key),
                ))
            }
        };
        Ok((
            reader.clone(),
            quote! {
                fn #reader<'de, D: ::serde::Deserializer<'de>>(
                    d: D,
                ) -> ::core::result::Result<#ty, D::Error> {
                    match ::volt_coerce::Reader::<#ty>::read(d)? {
                        ::core::option::Option::Some(value) => ::core::result::Result::Ok(value),
                        ::core::option::Option::None => #skipped,
                    }
                }
            },
        ))
    }

    /// Returns the owner's `Coerce` implementation, which reads a JSON object through the owner's
    /// `Deserialize` so the struct nests inside lists, options and maps.
    fn object_rule(&self) -> TokenStream {
        let owner = &self.owner;
        quote! {
            impl<'de> ::volt_coerce::Coerce<'de> for #owner {
                fn read_object<A: ::serde::de::MapAccess<'de>>(
                    object: A,
                ) -> ::core::result::Result<::core::option::Option<Self>, A::Error> {
                    <Self as ::serde::Deserialize<'de>>::deserialize(
                        ::serde::de::value::MapAccessDeserializer::new(object),
                    )
                    .map(::core::option::Option::Some)
                }
            }
        }
    }
}

/// The expansion for a type declared with `#[serde(from = "…")]` or
/// `#[serde(try_from = "…")]`: the source type is read leniently, then converted.
struct Twin;

impl Twin {
    /// Returns `owner`'s `Coerce` implementation: each scalar shape reads as `raw` and converts
    /// with `From`, or with `TryFrom` when `fallible`, whose error fails the document; an object is
    /// decoded with `owner`'s `Deserialize`.
    fn rule(owner: &Ident, raw: &Type, fallible: bool) -> TokenStream {
        let convert = if fallible {
            quote! {
                .map(|raw| {
                    <#owner as ::core::convert::TryFrom<#raw>>::try_from(raw)
                        .map_err(<E as ::serde::de::Error>::custom)
                })
                .transpose()?
            }
        } else {
            quote!(.map(::core::convert::Into::into))
        };
        let scalars = [
            (quote!(read_null), quote!(), quote!()),
            (quote!(read_bool), quote!(value: bool), quote!(value)),
            (quote!(read_i64), quote!(value: i64), quote!(value)),
            (quote!(read_u64), quote!(value: u64), quote!(value)),
            (quote!(read_f64), quote!(value: f64), quote!(value)),
            (quote!(read_str), quote!(value: &str), quote!(value)),
            (quote!(read_borrowed_str), quote!(value: &'de str), quote!(value)),
            (quote!(read_string), quote!(value: ::std::string::String), quote!(value)),
        ]
        .map(|(name, param, arg)| {
            quote! {
                fn #name<E: ::serde::de::Error>(#param) -> ::core::result::Result<::core::option::Option<Self>, E> {
                    ::core::result::Result::Ok(
                        <#raw as ::volt_coerce::Coerce<'de>>::#name(#arg)? #convert,
                    )
                }
            }
        });
        quote! {
            impl<'de> ::volt_coerce::Coerce<'de> for #owner
            where
                #raw: ::volt_coerce::Coerce<'de>,
            {
                #(#scalars)*

                fn read_object<A: ::serde::de::MapAccess<'de>>(
                    object: A,
                ) -> ::core::result::Result<::core::option::Option<Self>, A::Error> {
                    <Self as ::serde::Deserialize<'de>>::deserialize(
                        ::serde::de::value::MapAccessDeserializer::new(object),
                    )
                    .map(::core::option::Option::Some)
                }
            }
        }
    }
}

impl Lenient {
    /// Returns a branch that never runs but reads the owner's fields `names` through raw pointers,
    /// as serde's `pretend_used` does for a packed struct, or nothing when there are none.
    fn pretend(names: &[&Ident]) -> Option<TokenStream> {
        (!names.is_empty()).then(|| {
            quote! {
                if let ::core::option::Option::Some(__v) = ::core::option::Option::None::<&Self> {
                    #(let _ = ::core::ptr::addr_of!(__v.#names);)*
                }
            }
        })
    }

    /// Whether `ty` is a path whose last segment is `Option`.
    fn optional(ty: &Type) -> bool {
        matches!(ty, Type::Path(path) if path.qself.is_none()
            && path.path.segments.last().is_some_and(|last| last.ident == "Option"))
    }
}

impl Twin {
    /// Expands an enum, which needs `#[serde(from)]` or `#[serde(try_from)]` when it is not
    /// `custom`: derives serde's `Deserialize` and adds the lenient `Coerce` implementation.
    fn expand(mut item: ItemEnum) -> syn::Result<TokenStream> {
        let attrs = Serde::read(&item.attrs)?;
        let Some((raw, fallible)) = attrs.twin() else {
            return Err(syn::Error::new_spanned(
                &item.ident,
                "an enum needs `custom` or #[serde(from = \"..\")]",
            ));
        };
        let rule = Self::rule(&item.ident, &raw.parse()?, fallible);
        item.attrs
            .insert(0, parse_quote!(#[derive(::serde::Deserialize)]));
        Ok(quote! {
            #item

            const _: () = {
                #rule
            };
        })
    }
}

/// The expansion of `#[coerce(custom)]`, for a type that implements `Coerce` and `Default` by
/// hand.
struct Custom;

impl Custom {
    /// Returns `item` with a `Deserialize` implementation for `ident` that reads through the
    /// hand-written `Coerce` rules and falls back to `Default` on junk.
    fn expand(item: &Item, ident: &Ident) -> TokenStream {
        quote! {
            #item

            const _: () = {
                impl<'de> ::serde::Deserialize<'de> for #ident {
                    fn deserialize<D: ::serde::Deserializer<'de>>(
                        d: D,
                    ) -> ::core::result::Result<Self, D::Error> {
                        ::volt_coerce::Reader::coerce(d)
                    }
                }
            };
        }
    }
}

/// Registers `serde` as a helper attribute and generates nothing, so `#[serde(…)]` stays legal
/// on a struct whose `Deserialize` comes from `#[coerce]`. `#[coerce]` adds it itself; it is
/// not meant to be derived by hand.
#[proc_macro_derive(SerdeAttrs, attributes(serde))]
pub fn serde_attrs(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

/// Derives a lenient serde `Deserialize`: a field that arrives as junk, the wrong type or `null`
/// takes its default instead of failing the whole document.
///
/// On a struct with named fields every field reads through `volt_coerce::Coerce`. Junk falls to
/// the field's `#[serde(default)]` or `default = "…"`, then to the container's, and an `Option`
/// field reads it as `None`; with no default it is an error. A missing key follows serde, except
/// that an `Option` field defaults to `None`. Fields with `flatten`, `skip`,
/// `skip_deserializing`, `with` or `deserialize_with` are left to serde.
///
/// - `#[strict]` on a field keeps serde's strict decoding for it.
/// - `#[reject]` on a field still reads it leniently, but junk fails the document instead of
///   falling to the default.
/// - `#[coerce(custom)]` on a struct or enum with hand-written `Coerce` and `Default`
///   implementations derives only `Deserialize`, which falls back to `Default` on junk.
/// - `#[coerce(take_from)]` on a struct also generates `take_from(map)`, which removes the
///   struct's keys from a JSON object map and reads them as `Deserialize` would, leaving every
///   other key in the map. Fields with `alias`, `with`, `deserialize_with` or `rename(…)`, a
///   struct with `rename_all` and the `from`/`try_from` and `custom` forms refuse it.
/// - `#[coerce(after = path)]` runs `path(&mut value)` after decoding; it returns `()` or
///   `Result<(), E>`, and an error fails the document.
/// - With `#[serde(from = "…")]` or `#[serde(try_from = "…")]` on a struct or enum, the
///   source type is read leniently and converted. An enum without `custom` needs one of them.
/// - A bare `#[serde(default)]` with a `#[derive(Default)]` (taken to be std's) written below
///   `#[coerce]` gives each field that falls to it its type's default without building the
///   whole struct, unless the struct is `transparent`, gives a field a `= value` default or has a
///   `#[deprecated]` on itself or a field. Any other container default is built whole as serde
///   builds it, and again for each junk field that falls to it.
///
/// The annotated type also implements `Coerce`, so it nests leniently in lists, options and
/// maps. The generated code refers to `::volt_coerce`.
#[proc_macro_attribute]
pub fn coerce(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(error) => return error.into_compile_error().into(),
    };
    let item = parse_macro_input!(input as Item);
    if options.take_from && (options.custom || !matches!(item, Item::Struct(_))) {
        return syn::Error::new_spanned(
            item,
            "`take_from` applies to a struct read through `#[coerce]`",
        )
        .into_compile_error()
        .into();
    }
    let expanded = match (options.custom, item) {
        (true, item) => {
            let ident = match &item {
                Item::Struct(item) => item.ident.clone(),
                Item::Enum(item) => item.ident.clone(),
                other => {
                    return syn::Error::new_spanned(
                        other,
                        "#[coerce] applies to structs and enums",
                    )
                    .into_compile_error()
                    .into();
                }
            };
            Ok(Custom::expand(&item, &ident))
        }
        (false, Item::Struct(item)) => Lenient::expand(item, options.after, options.take_from),
        (false, Item::Enum(item)) => Twin::expand(item),
        (false, other) => Err(syn::Error::new_spanned(
            other,
            "#[coerce] applies to structs and enums",
        )),
    };
    expanded
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
