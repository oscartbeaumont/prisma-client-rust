use convert_case::{Case, Casing};
use quote::{__private::TokenStream, format_ident, quote};
use syn::Ident;

use crate::generator::dmmf::{Document, Method, Model, Type};

struct Outputs {
    pub fn_name: Ident,
    outputs: Vec<TokenStream>,
}

impl Outputs {
    pub fn new(model: &Model) -> Self {
        Self {
            fn_name: Self::get_fn_name(&model.name),
            outputs: model
                .fields
                .iter()
                .filter(|f| f.kind.include_in_struct())
                .map(|field| {
                    let field_name_string = &field.name;
                    quote!(Output::new(#field_name_string))
                })
                .collect(),
        }
    }

    pub fn quote(&self) -> TokenStream {
        let Self { fn_name, outputs } = self;

        quote! {
            fn #fn_name() -> Vec<Output> {
                vec![
                    #(#outputs),*
                ]
            }
        }
    }

    pub fn get_fn_name(model_name: &str) -> Ident {
        format_ident!("{}_outputs", model_name.to_case(Case::Snake))
    }
}

struct WhereParams {
    pub enum_name: Ident,
    pub unique_enum_name: Ident,
    variants: Vec<TokenStream>,
    match_arms: Vec<TokenStream>,
    unique_variants: Vec<TokenStream>,
    from_unique_match_arms: Vec<TokenStream>
}

impl WhereParams {
    pub fn new(model: &Model) -> Self {
        let enum_name = Self::get_enum_name(&model.name);
        let unique_enum_name = Self::get_unique_enum_name(&model.name);
        
        let mut params = Self {
            enum_name: enum_name.clone(),
            unique_enum_name: unique_enum_name.clone(),
            variants: vec![],
            unique_variants: vec![],
            match_arms: vec![],
            from_unique_match_arms: vec![]
        };

        for field in &model.fields {
            let field_type_string = field.field_type.string();
            let field_name_pascal = field.name.to_case(Case::Pascal);
            let field_name_string = &field.name;

            if field.kind.is_relation() {
                let actions = field.relation_methods();

                let field_type_where_param = format_ident!("{}WhereParam", field_type_string);

                for action in actions {
                    let action_name = action.name;
                    let variant_name = format_ident!("{}{}", field_name_pascal, &action_name);

                    params.add_variant(
                        quote!(#variant_name(Vec<#field_type_where_param>)),
                        quote! {
                            Self::#variant_name(value) =>
                                Field {
                                    name: #field_name_string.into(),
                                    fields: Some(vec![
                                        Field {
                                            name: "AND".into(),
                                            fields: Some(value.into_iter().map(|f| f.to_field()).collect()),
                                            ..Default::default()
                                        }
                                    ]),
                                    ..Default::default()
                                }
                        }
                    );
                }
            } else {
                let read_types = match Document::read_types()
                    .into_iter()
                    .find(|t| t.name == field.field_type.string())
                {
                    Some(mut t) => Type {
                        methods: {
                            t.methods.append(&mut vec![Method {
                                name: "Equals".into(),
                                action: "equals".into(),
                            }]);
                            t.methods
                        },
                        ..t
                    },
                    None => Type {
                        name: field.field_type.string().into(),
                        methods: vec![Method {
                            name: "Equals".into(),
                            action: "equals".into(),
                        }],
                    },
                };

                let field_type_value = field.field_type.value();
                let field_type_tokens = field.field_type.value_tokens();
                
                for m in read_types.methods {
                    let variant_name = format_ident!("{}{}", field_name_pascal, &m.name);
                    let method_action = m.action;
                    
                    let variant = quote!(#variant_name(#field_type_tokens));
                    let match_arm = quote! {
                        Self::#variant_name(value) => Field {
                            name: #field_name_string.into(),
                            fields: Some(vec![
                                Field {
                                    name: #method_action.into(),
                                    value: Some(serde_json::to_value(value).unwrap()),
                                    ..Default::default()
                                }
                            ]),
                            ..Default::default()
                        }
                    };
                
                    if field.is_unique || field.is_id {
                        params.unique_variants.push(variant.clone());
                        params.from_unique_match_arms.push(quote! {
                            #unique_enum_name::#variant_name(value) => Self::#variant_name(value)
                        });
                    }
                    
                    params.add_variant(variant, match_arm);
                }
            }
        }

        for op in Document::operators() {
            let name_ident = format_ident!("{}", op.name);
            let action = &op.action;

            params.add_variant(
                quote!(#name_ident(Vec<#enum_name>)),
                quote! {
                    Self::#name_ident(value) =>
                        Field {
                            name: #action.into(),
                            list: true,
                            wrap_list: true,
                            fields: Some(value.into_iter().map(|f| f.to_field()).collect()),
                            ..Default::default()
                        }
                },
            );
        }

        params
    }

    fn add_variant(&mut self, variant: TokenStream, match_arm: TokenStream) {
        self.variants.push(variant);
        self.match_arms.push(match_arm);
    }

    pub fn quote(&self) -> TokenStream {
        let Self {
            variants,
            unique_variants,
            match_arms,
            enum_name,
            unique_enum_name,
            from_unique_match_arms
        } = self;

        quote! {
            pub enum #enum_name {
                #(#variants),*
            }

            impl #enum_name {
                pub fn to_field(self) -> Field {
                    match self {
                        #(#match_arms),*
                    }
                }
            }
            
            impl From<Operator<Self>> for #enum_name {
                fn from(op: Operator<Self>) -> Self {
                    match op {
                        Operator::Not(value) => Self::Not(value),
                        Operator::And(value) => Self::And(value),
                        Operator::Or(value) => Self::Or(value)
                    }
                }
            }
        
            pub enum #unique_enum_name {
                #(#unique_variants),*
            }
            
            impl From<#unique_enum_name> for #enum_name {
                fn from(op: #unique_enum_name) -> Self {
                    match op {
                        #(#from_unique_match_arms),*
                    }
                }
            }
        }
    }

    pub fn get_enum_name(model_name: &str) -> Ident {
        format_ident!("{}WhereParam", model_name.to_case(Case::Pascal))
    }
    pub fn get_unique_enum_name(model_name: &str) -> Ident {
        format_ident!("{}UniqueWhereParam", model_name.to_case(Case::Pascal))
    }
}

struct WithParams {
    pub enum_name: Ident,
    pub with_fn: TokenStream,
    variants: Vec<TokenStream>,
    match_arms: Vec<TokenStream>,
}

impl WithParams {
    pub fn new(model: &Model, outputs: &Outputs) -> Self {
        let model_name_pascal_string = model.name.to_case(Case::Pascal);
        let enum_name = format_ident!("{}WithParam", &model_name_pascal_string);

        let mut params = Self {
            with_fn: quote! {
                pub fn with(mut self, param: #enum_name) -> Self {
                    self.with_params.push(param);
                    self
                }
            },
            enum_name,
            variants: vec![],
            match_arms: vec![],
        };

        model.fields.iter()
            .filter(|f| f.kind.is_relation())
            .for_each(|field| {
                let field_name_string = &field.name;
                let relation_type_string = field.field_type.value();

                let field_name_pascal = format_ident!("{}", &field.name.to_case(Case::Pascal));
                let relation_outputs_fn = Outputs::get_fn_name(&relation_type_string);
                let relation_where_param = WhereParams::get_enum_name(&relation_type_string);

                if field.is_list {
                    params.add_variant(
                        quote!(#field_name_pascal(Vec<#relation_where_param>)),
                        quote! {
                            Self::#field_name_pascal(where_params) => Output {
                                name: #field_name_string.into(),
                                outputs: #relation_outputs_fn(),
                                inputs: if where_params.len() > 0 {
                                    vec![Input {
                                        name: "where".into(),
                                        fields: where_params.into_iter().map(|f| f.to_field()).collect(),
                                        ..Default::default()
                                    }]
                                } else { vec![] },
                                ..Default::default()
                            }
                        }
                    )
                } else {
                    params.add_variant(
                        quote!(#field_name_pascal),
                        quote! {
                            Self::#field_name_pascal => Output {
                                name: #field_name_string.into(),
                                outputs: #relation_outputs_fn(),
                                ..Default::default()
                            }
                        }
                    )
                }
            });

        params
    }

    fn add_variant(&mut self, variant: TokenStream, match_arm: TokenStream) {
        self.variants.push(variant);
        self.match_arms.push(match_arm);
    }

    pub fn quote(&self) -> TokenStream {
        let Self {
            variants,
            match_arms,
            enum_name: name,
            ..
        } = self;

        quote! {
            pub enum #name {
                #(#variants),*
            }

            impl #name {
                pub fn to_output(self) -> Output {
                    match self {
                        #(#match_arms),*
                    }
                }
            }
        }
    }
}

struct SetParams {
    pub enum_name: Ident,
    variants: Vec<TokenStream>,
    match_arms: Vec<TokenStream>,
}

impl SetParams {
    pub fn new(model: &Model) -> Self {
        let model_name_pascal = format_ident!("{}", model.name.to_case(Case::Pascal));

        let mut params = Self {
            enum_name: format_ident!("{}SetParam", model_name_pascal),
            variants: vec![],
            match_arms: vec![],
        };

        for field in &model.fields {
            let field_name_string = &field.name;
            let field_name_pascal = field.name.to_case(Case::Pascal);
            let field_type_string = field.field_type.value();
            
            let field_type_tokens = field.field_type.value_tokens();
            let (set_variant, relation_where_param) = if field.kind.is_relation() {
                (format_ident!("Link{}", &field_name_pascal), Some(WhereParams::get_enum_name(&field_type_string)))
            } else {
                (format_ident!("{}", &field_name_pascal), None)
            };

            let (variant, match_arm) = match (field.kind.include_in_struct(), field.is_list) {
                (true, _) => (
                    quote!(#set_variant(#field_type_tokens)),
                    quote! {
                        Self::#set_variant(value) => Field {
                            name: #field_name_string.into(),
                            value: Some(serde_json::to_value(value).unwrap()),
                            ..Default::default()
                        }
                    },
                ),
                (_, true) => (
                    quote!(#set_variant(Vec<#relation_where_param>)),
                    quote! {
                        Self::#set_variant(where_params) => Field {
                            name: #field_name_string.into(),
                            fields: Some(vec![
                                Field {
                                    name: "connect".into(),
                                    fields: Some(transform_equals(
                                        where_params
                                            .into_iter()
                                            .map(|item| item.to_field())
                                            .collect()
                                    )),
                                    list: true,
                                    wrap_list: true,
                                    ..Default::default()
                                }
                            ]),
                            ..Default::default()
                        }
                    },
                ),
                (_, false) => (
                    quote!(#set_variant(#relation_where_param)),
                    quote! {
                        Self::#set_variant(where_param) => Field {
                            name: #field_name_string.into(),
                            fields: Some(vec![
                                Field {
                                    name: "connect".into(),
                                    fields: Some(transform_equals(vec![
                                        where_param.to_field()
                                    ])),
                                    ..Default::default()
                                }
                            ]),
                            ..Default::default()
                        }
                    },
                ),
            };

            params.add_variant(variant, match_arm);
            
            if field.kind.is_relation() {
                let unlink_variant = format_ident!("Unlink{}", &field_name_pascal);
                
                if field.is_list {
                    params.add_variant(
                        quote!(#unlink_variant(Vec<#relation_where_param>)),
                        quote! {
                            Self::#unlink_variant(where_params) => Field {
                                name: #field_name_string.into(),
                                fields: Some(vec![
                                    Field {
                                        name: "disconnect".into(),
                                        list: true,
                                        wrap_list: true,
                                        fields: Some(transform_equals(
                                            where_params
                                                .into_iter()
                                                .map(|item| item.to_field())
                                                .collect()
                                        )),
                                        ..Default::default()
                                    }
                                ]),
                                ..Default::default()
                            }
                        }
                    );
                } else if !field.is_required {
                    params.add_variant(
                        quote!(#unlink_variant),
                        quote! {
                            Self::#unlink_variant => Field {
                                name: #field_name_string.into(),
                                fields: Some(vec![Field {
                                    name: "disconnect".into(),
                                    value: Some(true.into()),
                                    ..Default::default()
                                }]),
                                ..Default::default()
                            }
                        }
                    )
                }
            }
        }

        params
    }

    fn add_variant(&mut self, variant: TokenStream, match_arm: TokenStream) {
        self.variants.push(variant);
        self.match_arms.push(match_arm);
    }

    pub fn quote(&self) -> TokenStream {
        let Self {
            variants,
            match_arms,
            enum_name,
        } = self;

        quote! {
            pub enum #enum_name {
                #(#variants),*
            }

            impl #enum_name {
                pub fn to_field(self) -> Field {
                    match self {
                        #(#match_arms),*
                    }
                }
            }
        }
    }
}

struct OrderByParams {
    pub enum_name: Ident,
    pub order_by_fn: TokenStream,
    variants: Vec<TokenStream>,
    match_arms: Vec<TokenStream>,
}

impl OrderByParams {
    pub fn new(model: &Model) -> Self {
        let model_name_pascal = format_ident!("{}", model.name.to_case(Case::Pascal));
        let enum_name = format_ident!("{}OrderByParam", model_name_pascal);
        
        let mut params = Self {
            enum_name: enum_name.clone(),
            order_by_fn: quote! {
                pub fn order_by(mut self, param: #enum_name) -> Self {
                    self.order_by_params.push(param);
                    self
                }
            },
            variants: vec![],
            match_arms: vec![],
        };
        
        for field in &model.fields {
            if field.kind.is_relation() {
                continue;
            }
            
            let field_name_string = &field.name;
            let variant_name = format_ident!("{}", field.name.to_case(Case::Pascal));
            
            params.add_variant(quote!(#variant_name(Direction)),
            quote! {
                Self::#variant_name(direction) => Field {
                    name: #field_name_string.into(),
                    value: Some(serde_json::to_value(direction).unwrap()),
                    ..Default::default()
                }
            });
        }
        
        params
    }
    
    fn add_variant(&mut self, variant: TokenStream, match_arm: TokenStream) {
        self.variants.push(variant);
        self.match_arms.push(match_arm);
    }
    
    pub fn quote(&self) -> TokenStream {
        let Self {
            variants,
            match_arms,
            enum_name,
            ..
        } = self;
        
        quote! {
            pub enum #enum_name {
                #(#variants),*
            }
            
            impl #enum_name {
                pub fn to_field(self) -> Field {
                    match self {
                        #(#match_arms),*
                    }
                }
            }
        }
    }
}

struct PaginationParams {
    pub cursor_enum_name: Ident,
    pub pagination_fns: TokenStream,
    cursor_variants: Vec<TokenStream>,
    cursor_match_arms: Vec<TokenStream>
}

impl PaginationParams {
    pub fn new(model: &Model) -> Self {
        let model_name_pascal = format_ident!("{}", model.name.to_case(Case::Pascal));
        let cursor_enum_name = format_ident!("{}Cursor", model_name_pascal);
        
        let pagination_fns = quote! {
            pub fn skip(mut self, skip: usize) -> Self {
                self.query.inputs.push(Input {
                    name: "skip".into(),
                    value: Some(serde_json::to_value(skip).unwrap()),
                    ..Default::default()
                });
                self
            }
            
            pub fn take(mut self, take: usize) -> Self {
                self.query.inputs.push(Input {
                    name: "take".into(),
                    value: Some(serde_json::to_value(take).unwrap()),
                    ..Default::default()
                });
                self
            }
            
            pub fn cursor(mut self, cursor: #cursor_enum_name) -> Self {
                self.query.inputs.push(Input {
                    name: "cursor".into(),
                    fields: vec![cursor.to_field()],
                    ..Default::default()
                });
                self
            }
        };
        
        let mut cursor_variants = vec![];
        let mut cursor_match_arms = vec![];
        
        for f in model.fields.iter().filter(|f| !f.kind.is_relation()) {
            let variant_name = format_ident!("{}", f.name.to_case(Case::Pascal));
            let variant_type = format_ident!("{}", f.field_type.value());
            let field_name_string = &f.name;
            
            cursor_variants.push(quote! {
                #variant_name(#variant_type)
            });
            
            cursor_match_arms.push(quote! {
                Self::#variant_name(value) => Field {
                    name: #field_name_string.into(),
                    value: Some(serde_json::to_value(value).unwrap()),
                    ..Default::default()
                }
            })
        }
        
        Self {
            cursor_enum_name,
            pagination_fns,
            cursor_variants,
            cursor_match_arms
        }
    }
    
    pub fn quote(&self) -> TokenStream {
        let Self {
            cursor_enum_name,
            pagination_fns,
            cursor_variants,
            cursor_match_arms
        } = self;
        
        quote! {
            pub enum #cursor_enum_name {
                #(#cursor_variants),*
            }
            
            impl #cursor_enum_name {
                fn to_field(self) -> Field {
                    match self {
                        #(#cursor_match_arms),*
                    }
                }
            }
        }
    }
}

struct QueryStructs {
    pub name: Ident,
    methods: Vec<TokenStream>,
    field_structs: Vec<TokenStream>,
}

impl QueryStructs {
    pub fn new(
        model: &Model,
        set_params: &SetParams,
        where_params: &WhereParams,
        with_params: &WithParams,
        order_by_params: &OrderByParams,
    ) -> Self {
        let model_name_pascal = format_ident!("{}", model.name.to_case(Case::Pascal));

        let model_set_param = &set_params.enum_name;
        let model_where_param = &where_params.enum_name;
        let model_unique_where_param = &where_params.unique_enum_name;

        let methods = model
            .fields
            .iter()
            .map(|field| {
                let field_method_name = format_ident!("{}", field.name.to_case(Case::Snake));
                let field_struct_name = format_ident!(
                    "{}{}Field",
                    model.name.to_case(Case::Pascal),
                    field.name.to_case(Case::Pascal)
                );

                quote! {
                    pub fn #field_method_name() -> #field_struct_name {
                        #field_struct_name {}
                    }
                }
            })
            .collect::<Vec<_>>();

        let field_structs = model
            .fields
            .iter()
            .map(|field| {
                let field_name_pascal = format_ident!("{}", field.name.to_case(Case::Pascal));
                let field_struct_name =
                    format_ident!("{}{}Field", model_name_pascal, &field_name_pascal);
                let field_type_string = field.field_type.value();
                let field_type = field.field_type.value_tokens();

                let mut field_struct_fns = if field.kind.is_relation() {
                let relation_where_param = WhereParams::get_enum_name(&field.field_type.value());
                    let methods = field.relation_methods();

                    methods
                        .iter()
                        .map(|m| {
                            let variant_name = format_ident!(
                                "{}{}",
                                &field_name_pascal,
                                m.name.to_case(Case::Pascal)
                            );
                            let method_name_snake = format_ident!("{}", m.name.to_case(Case::Snake));

                            quote! {
                                pub fn #method_name_snake(&self, value: Vec<#relation_where_param>) -> #model_where_param {
                                    #model_where_param::#variant_name(value)
                                }
                            }
                        })
                        .collect::<Vec<_>>()

                } else {
                    match Document::read_types()
                        .into_iter()
                        .find(|t| t.name == field.field_type.string()) {
                        Some(read_types) => 
                        read_types
                            .methods
                            .iter()
                            .map(|m| {
                                let variant_name = format_ident!(
                                    "{}{}",
                                    &field_name_pascal,
                                    m.name
                                );
                                let method_name = format_ident!("{}", &m.name.to_case(Case::Snake));
                                
                                quote! {
                                    pub fn #method_name(&self, value: #field_type) -> #model_where_param {
                                        #model_where_param::#variant_name(value)
                                    }
                                }
                            })
                            .collect::<Vec<_>>(),
                        None => vec![]
                    }
                };
                
                if !field.kind.is_relation() {
                    let variant_name = format_ident!(
                        "{}Equals",
                        &field_name_pascal,
                    );
                    field_struct_fns.push(match (field.is_unique || field.is_id) {
                       true => quote! {
                            pub fn equals<T: From<#model_unique_where_param>>(&self, value: #field_type) -> T {
                                #model_unique_where_param::#variant_name(value).into()
                            }
                        },
                        false => quote! {
                            pub fn equals(&self, value: #field_type) -> #model_where_param {
                                #model_where_param::#variant_name(value).into()
                            }
                        }
                    });
                }
                
                let field_set_struct = if field.kind.is_relation() {
                    let relation_where_param = WhereParams::get_enum_name(&field.field_type.value());
                    let field_link_struct_name = format_ident!("{}Link{}", model_name_pascal, &field_name_pascal);
                    let link_variant = format_ident!("Link{}", &field_name_pascal);
                    let unlink_varaint = format_ident!("Unlink{}", &field_name_pascal);

                    let with_enum = &with_params.enum_name;

                    if field.is_list {                        
                        field_struct_fns.push(quote! {
                            pub fn link<T: From<#field_link_struct_name>>(&self, value: Vec<#relation_where_param>) -> T {
                                #field_link_struct_name(value).into()
                            }
                            
                            pub fn unlink(&self, params: Vec<#relation_where_param>) -> #model_set_param {
                                #model_set_param::#unlink_varaint(params)
                            }

                            pub fn fetch(&self, params: Vec<#relation_where_param>) -> #with_enum {
                                #with_enum::#field_name_pascal(params)
                            }
                        });

                        quote! {
                            pub struct #field_link_struct_name(Vec<#relation_where_param>);

                            impl From<#field_link_struct_name> for #model_set_param {
                                fn from(value: #field_link_struct_name) -> Self {
                                    Self::#link_variant(value.0.into_iter().map(|v| v.into()).collect())
                                }
                            }
                        }
                    } else {
                        let unlink_fn = if !field.is_required {
                            Some(quote! {
                                pub fn unlink(&self) -> #model_set_param {
                                    #model_set_param::#unlink_varaint
                                }
                            })
                        } else { None };
                        
                        field_struct_fns.push(quote! {
                            pub fn link<T: From<#field_link_struct_name>>(&self, value: #relation_where_param) -> T {
                                #field_link_struct_name(value).into()
                            }

                            pub fn fetch(&self) -> #with_enum {
                                #with_enum::#field_name_pascal
                            }
                        
                            #unlink_fn
                        });

                        quote! {
                            pub struct #field_link_struct_name(#relation_where_param);

                            impl From<#field_link_struct_name> for #model_set_param {
                                fn from(value: #field_link_struct_name) -> Self {
                                    Self::#link_variant(value.0)
                                }
                            }
                        }
                    }
                } else {
                    let field_set_struct_name = format_ident!("{}Set{}", model_name_pascal, &field_name_pascal);

                    field_struct_fns.push(quote! {
                        pub fn set<T: From<#field_set_struct_name>>(&self, value: #field_type) -> T {
                            #field_set_struct_name(value).into()
                        }
                    });
                    
                    let order_by_enum = &order_by_params.enum_name;
                    field_struct_fns.push(quote! {
                        pub fn order(&self, direction: Direction) -> #order_by_enum {
                            #order_by_enum::#field_name_pascal(direction)
                        }
                    });

                    quote! {
                        pub struct #field_set_struct_name(#field_type);

                        impl From<#field_set_struct_name> for #model_set_param {
                            fn from(value: #field_set_struct_name) -> Self {
                                Self::#field_name_pascal(value.0)
                            }
                        }
                    }
                };

                quote! {
                    pub struct #field_struct_name {}

                    #field_set_struct

                    impl #field_struct_name {
                        #(#field_struct_fns)*
                    }
                }
            })
            .collect();

        Self {
            name: format_ident!("{}", model.name.to_case(Case::Pascal)),
            methods,
            field_structs,
        }
    }

    pub fn quote(&self) -> TokenStream {
        let Self {
            name,
            methods,
            field_structs,
        } = self;

        quote! {
            pub struct #name;

            impl #name {
                #(#methods)*
            }

            #(#field_structs)*
        }
    }
}

struct DataStruct {
    pub name: Ident,
    fields: Vec<TokenStream>,
    relation_accessors: Vec<TokenStream>,
}

impl DataStruct {
    pub fn new(model: &Model) -> Self {
        let fields = model
            .fields
            .iter()
            .map(|field| {
                let field_name_string = &field.name;
                let field_name_snake = format_ident!("{}", field.name.to_case(Case::Snake));
                let field_type_string = field.field_type.value();

                if field.kind.is_relation() {
                    let field_type = Self::get_struct_name(&field_type_string);

                    match (field.is_list, field.is_required) {
                        (true, _) => quote! {
                           #[serde(rename = #field_name_string)]
                           #field_name_snake: Option<Vec<#field_type>>
                        },
                        (_, true) => quote! {
                            #[serde(rename = #field_name_string)]
                            #field_name_snake: Box<Option<#field_type>>
                        },
                        (_, false) => quote! {
                            #[serde(rename = #field_name_string)]
                            pub #field_name_snake: Box<Option<#field_type>>
                        },
                    }
                } else {
                    let field_type = field.field_type.value_tokens();

                    match (field.is_list, field.is_required) {
                        (true, _) => quote! {
                            #[serde(rename = #field_name_string)]
                            pub #field_name_snake: Vec<#field_type>
                        },
                        (_, true) => quote! {
                            #[serde(rename = #field_name_string)]
                            pub #field_name_snake: #field_type
                        },
                        (_, false) => quote! {
                            #[serde(rename = #field_name_string)]
                            pub #field_name_snake: Option<#field_type>
                        },
                    }
                }
            })
            .collect();

        let relation_accessors = model
            .fields
            .iter()
            .filter(|f| f.kind.is_relation())
            .map(|field| {
                let field_name_snake = format_ident!("{}", field.name.to_case(Case::Snake));
                let field_type = DataStruct::get_struct_name(&field.field_type.value());

                let return_type = match field.is_list {
                    true => quote!(Vec<#field_type>),
                    false => quote!(#field_type),
                };

                if field.is_required {
                    let err = format!(
                        "Attempted to access {} but did not fetch it using the .with() syntax",
                        field_name_snake
                    );

                    quote! {
                        pub fn #field_name_snake(&self) -> Result<&#return_type, String> {
                            match self.#field_name_snake.as_ref() {
                                Some(v) => Ok(v),
                                None => Err(#err.to_string()),
                            }
                        }
                    }
                } else {
                    // TODO: Figure out double option to allow for null check
                    // println!("attempted to access optional relation {} but did not fetch it using the .with() syntax", field_name_snake);

                    quote! {
                        pub fn #field_name_snake(&self) -> Option<&#return_type> {
                            self.#field_name_snake.as_ref().as_ref()
                        }
                    }
                }
            })
            .collect();

        Self {
            name: Self::get_struct_name(&model.name),
            fields,
            relation_accessors,
        }
    }

    pub fn quote(&self) -> TokenStream {
        let Self {
            name,
            fields,
            relation_accessors,
        } = self;

        quote! {
            #[derive(Debug, Clone, Serialize, Deserialize)]
            pub struct #name {
                #(#fields),*
            }

            impl #name {
                #(#relation_accessors)*
            }
        }
    }

    pub fn get_struct_name(model_name: &str) -> Ident {
        format_ident!("{}Data", model_name.to_case(Case::Pascal))
    }
}

struct Actions<'a> {
    struct_name: Ident,
    model_name_pascal_string: String,
    data_struct_name: &'a Ident,
    where_param_enum: &'a Ident,
    unique_where_param_enum: &'a Ident,
    set_param_enum: &'a Ident,
    order_by_enum: &'a Ident,
    outputs_fn_name: &'a Ident,
    order_by_fn: &'a TokenStream,
    with_param_enum: &'a Ident,
    with_fn: &'a TokenStream,
    pagination_fns: &'a TokenStream,
    required_args: Vec<TokenStream>,
    required_arg_pushes: Vec<TokenStream>,
    required_tuple_args: Vec<TokenStream>,
}

impl<'a> Actions<'a> {
    pub fn new(
        model: &Model,
        where_params: &'a WhereParams,
        set_params: &'a SetParams,
        with_params: &'a WithParams,
        outputs: &'a Outputs,
        data_struct: &'a DataStruct,
        order_by_params: &'a OrderByParams,
        pagination_params: &'a PaginationParams
    ) -> Self {
        let model_name_pascal_string = model.name.to_case(Case::Pascal);
        let set_param_enum = &set_params.enum_name;

        let required_args = model
            .fields
            .iter()
            .filter(|f| f.required_on_create())
            .map(|f| {
                let arg_name = format_ident!("{}", &f.name.to_case(Case::Snake));
                let arg_type = match f.kind.is_relation() {
                    true => format_ident!(
                        "{}Link{}",
                        model_name_pascal_string,
                        f.name.to_case(Case::Pascal)
                    ),
                    false => format_ident!(
                        "{}Set{}",
                        model_name_pascal_string,
                        f.name.to_case(Case::Pascal)
                    )
                };
                
                quote! {
                    #arg_name: #arg_type,
                }
            })
            .collect();

        let required_arg_pushes = model
            .fields
            .iter()
            .filter(|f| f.required_on_create())
            .map(|f| {
                let arg_name = format_ident!("{}", &f.name.to_case(Case::Snake));
                quote! {
                    input_fields.push(#set_param_enum::from(#arg_name).to_field());
                }
            })
            .collect();
            
        let required_tuple_args = model
            .fields
            .iter()
            .filter(|f| f.required_on_create())
            .map(|f| {
                let arg_type = match f.kind.is_relation() {
                    true => format_ident!(
                        "{}Link{}",
                        model_name_pascal_string,
                        f.name.to_case(Case::Pascal)
                    ),
                    false => format_ident!(
                        "{}Set{}",
                        model_name_pascal_string,
                        f.name.to_case(Case::Pascal)
                    )
                };
                
                quote! {
                    #arg_type,
                }
            })
            .collect();

        Self {
            struct_name: format_ident!("{}Actions", &model_name_pascal_string),
            data_struct_name: &data_struct.name,
            model_name_pascal_string: model.name.to_case(Case::Pascal),
            where_param_enum: &where_params.enum_name,
            unique_where_param_enum: &where_params.unique_enum_name,
            set_param_enum,
            order_by_enum: &order_by_params.enum_name,
            order_by_fn: &order_by_params.order_by_fn,
            outputs_fn_name: &outputs.fn_name,
            with_param_enum: &with_params.enum_name,
            with_fn: &with_params.with_fn,
            pagination_fns: &pagination_params.pagination_fns,
            required_args,
            required_arg_pushes,
            required_tuple_args
        }
    }

    pub fn quote(&self) -> TokenStream {
        let Self {
            struct_name,
            data_struct_name,
            model_name_pascal_string,
            where_param_enum,
            unique_where_param_enum,
            set_param_enum,
            order_by_enum,
            order_by_fn,
            outputs_fn_name,
            with_param_enum,
            with_fn,
            pagination_fns,
            required_args,
            required_tuple_args,
            required_arg_pushes,
        } = self;

        let model_create_one = format_ident!("{}CreateOne", model_name_pascal_string);
        let model_find_first = format_ident!("{}FindFirst", model_name_pascal_string);
        let model_find_unique = format_ident!("{}FindUnique", model_name_pascal_string);
        let model_find_many = format_ident!("{}FindMany", model_name_pascal_string);
        let model_update_unique = format_ident!("{}UpdateUnique", model_name_pascal_string);
        let model_update_many = format_ident!("{}UpdateMany", model_name_pascal_string);
        let model_delete = format_ident!("{}Delete", model_name_pascal_string);
        
        quote! {
            pub struct #model_find_many<'a> {
                query: Query<'a>,
                order_by_params: Vec<#order_by_enum>,
                with_params: Vec<#with_param_enum>
            }

            impl<'a> #model_find_many<'a> {
                pub async fn exec(self) -> QueryResult<Vec<#data_struct_name>> {
                    let Self {
                        mut query,
                        order_by_params,
                        with_params
                    } = self;
                    
                    if order_by_params.len() > 0 {
                        query.inputs.push(Input {
                            name: "orderBy".into(),
                            fields: order_by_params
                                .into_iter()
                                .map(|f| f.to_field())
                                .collect(),
                            ..Default::default()
                        });
                    }
                    
                    query.outputs.extend(with_params
                        .into_iter()
                        .map(|f| f.to_output())
                        .collect::<Vec<_>>());
                    
                    query.perform::<Vec<#data_struct_name>>().await
                }

                pub fn delete(self) -> #model_delete<'a> {
                    #model_delete {
                        query: Query {
                            operation: "mutation".into(),
                            method: "deleteMany".into(),
                            model: #model_name_pascal_string.into(),
                            outputs: vec! [
                                Output::new("count"),
                            ],
                            ..self.query
                        }
                    }
                }

                pub fn update(mut self, params: Vec<#set_param_enum>) -> #model_update_many<'a> {
                    self.query.inputs.push(Input {
                        name: "data".into(),
                        fields: params
                            .into_iter()
                            .map(|param| {
                                let mut field = param.to_field();

                                if let Some(value) = field.value {
                                    field.fields = Some(vec![Field {
                                        name: "set".into(),
                                        value: Some(value),
                                        ..Default::default()
                                    }]);
                                    field.value = None;
                                }

                                field
                            })
                            .collect(),
                        ..Default::default()
                    });

                    #model_update_many {
                        query: Query {
                            operation: "mutation".into(),
                            method: "updateMany".into(),
                            ..self.query
                        },
                        with_params: vec![]
                    }
                }
                
                #order_by_fn

                #with_fn
                
                #pagination_fns
            }

            pub struct #model_find_first<'a> {
                query: Query<'a>,
                order_by_params: Vec<#order_by_enum>,
                with_params: Vec<#with_param_enum>
            }

            impl<'a> #model_find_first<'a> {
                pub async fn exec(self) -> QueryResult<#data_struct_name> {
                    let Self {
                        mut query,
                        order_by_params,
                        with_params
                    } = self;
                    
                    if order_by_params.len() > 0 {
                        query.inputs.push(Input {
                            name: "orderBy".into(),
                            fields: order_by_params
                                .into_iter()
                                .map(|f| f.to_field())
                                .collect(),
                            ..Default::default()
                        });
                    }
                    
                    query.outputs.extend(with_params
                        .into_iter()
                        .map(|f| f.to_output())
                        .collect::<Vec<_>>());
                    
                    query.perform::<#data_struct_name>().await
                }

                #with_fn
                
                #order_by_fn
                
                #pagination_fns
            }

            pub struct #model_find_unique<'a> {
                query: Query<'a>,
                with_params: Vec<#with_param_enum>
            }

            impl<'a> #model_find_unique<'a> {
                pub async fn exec(self) -> QueryResult<#data_struct_name> {
                    let Self {
                        mut query,
                        with_params
                    } = self;
                    
                    query.outputs.extend(with_params
                        .into_iter()
                        .map(|f| f.to_output())
                        .collect::<Vec<_>>());
                    
                    query.perform::<#data_struct_name>().await
                }

                pub fn delete(self) -> #model_delete<'a> {
                    #model_delete {
                        query: Query {
                            operation: "mutation".into(),
                            method: "deleteOne".into(),
                            model: #model_name_pascal_string.into(),
                            ..self.query
                        }
                    }
                }

                pub fn update(mut self, params: Vec<#set_param_enum>) -> #model_update_unique<'a> {
                    self.query.inputs.push(Input {
                        name: "data".into(),
                        fields: params
                            .into_iter()
                            .map(|param| {
                                let mut field = param.to_field();

                                if let Some(value) = field.value {
                                    field.fields = Some(vec![Field {
                                        name: "set".into(),
                                        value: Some(value),
                                        ..Default::default()
                                    }]);
                                    field.value = None;
                                }

                                field
                            })
                            .collect(),
                        ..Default::default()
                    });

                    #model_update_unique {
                        query: Query {
                            operation: "mutation".into(),
                            method: "updateOne".into(),
                            ..self.query
                        },
                        with_params: vec![]
                    }
                }

                #with_fn
            }

            pub struct #model_create_one<'a> {
                query: Query<'a>,
            }

            impl<'a> #model_create_one<'a> {
                pub async fn exec(self) -> QueryResult<#data_struct_name> {
                    self.query.perform::<#data_struct_name>().await
                }
            }

            pub struct #model_update_unique<'a> {
                query: Query<'a>,
                with_params: Vec<#with_param_enum>
            }

            impl<'a> #model_update_unique<'a> {
                pub async fn exec(self) -> QueryResult<#data_struct_name> {
                    self.query.perform::<#data_struct_name>().await
                }

                #with_fn
            }

            pub struct #model_update_many<'a> {
                query: Query<'a>,
                with_params: Vec<#with_param_enum>
            }

            impl<'a> #model_update_many<'a> {
                pub async fn exec(self) -> QueryResult<Vec<#data_struct_name>> {
                    self.query.perform::<Vec<#data_struct_name>>().await
                }

                #with_fn
            }

            pub struct #model_delete<'a> {
                query: Query<'a>
            }

            impl<'a> #model_delete<'a> {
                pub async fn exec(self) -> QueryResult<isize> {
                    self.query.perform::<DeleteResult>().await.map(|r| r.count)
                }
            }

            pub struct #struct_name<'a> {
                client: &'a PrismaClient,
            }

            impl<'a> #struct_name<'a> {
                // TODO: Dedicated unique field
                pub fn find_unique(&self, param: #unique_where_param_enum) -> #model_find_unique {
                    let param: #where_param_enum = param.into();
                    let fields = transform_equals(vec![param.to_field()]);

                    let query = Query {
                        ctx: QueryContext::new(&self.client.executor, self.client.query_schema.clone()),
                        name: String::new(),
                        operation: "query".into(),
                        method: "findUnique".into(),
                        model: #model_name_pascal_string.into(),
                        outputs: #outputs_fn_name(),
                        inputs: vec![Input {
                            name: "where".into(),
                            fields,
                            ..Default::default()
                        }]
                    };

                    #model_find_unique { 
                        query,
                        with_params: vec![]
                    }
                }

                pub fn find_first(&self, params: Vec<#where_param_enum>) -> #model_find_first {
                    let where_fields: Vec<Field> = params.into_iter().map(|param|
                        param.to_field()
                    ).collect();

                    let inputs = if where_fields.len() > 0 {
                        vec![Input {
                            name: "where".into(),
                            fields: vec![Field {
                                name: "AND".into(),
                                list: true,
                                wrap_list: true,
                                fields: Some(where_fields),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }]
                    } else {
                        Vec::new()
                    };

                    let query = Query {
                        ctx: QueryContext::new(&self.client.executor, self.client.query_schema.clone()),
                        name: String::new(),
                        operation: "query".into(),
                        method: "findFirst".into(),
                        model: #model_name_pascal_string.into(),
                        outputs: #outputs_fn_name(),
                        inputs
                    };

                    #model_find_first { query, order_by_params: vec![], with_params: vec![] }
                }

                pub fn find_many(&self, params: Vec<#where_param_enum>) -> #model_find_many {
                    let where_fields: Vec<Field> = params.into_iter().map(|param|
                        param.to_field()
                    ).collect();

                    let inputs = if where_fields.len() > 0 {
                        vec![Input {
                            name: "where".into(),
                            fields: where_fields,
                            ..Default::default()
                        }]
                    } else {
                        Vec::new()
                    };

                    let query = Query {
                        ctx: QueryContext::new(&self.client.executor, self.client.query_schema.clone()),
                        name: String::new(),
                        operation: "query".into(),
                        method: "findMany".into(),
                        model: #model_name_pascal_string.into(),
                        outputs: #outputs_fn_name(),
                        inputs
                    };

                    #model_find_many { query, order_by_params: vec![], with_params: vec![] }
                }

                pub fn create_one(&self, #(#required_args)* params: Vec<#set_param_enum>) -> #model_create_one {
                    let mut input_fields = params.into_iter().map(|p| p.to_field()).collect::<Vec<_>>();

                    #(#required_arg_pushes)*

                    let query = Query {
                        ctx: QueryContext::new(&self.client.executor, self.client.query_schema.clone()),
                        name: String::new(),
                        operation: "mutation".into(),
                        method: "createOne".into(),
                        model: #model_name_pascal_string.into(),
                        outputs: #outputs_fn_name(),
                        inputs: vec![Input {
                            name: "data".into(),
                            fields: input_fields,
                            ..Default::default()
                        }]
                    };

                    #model_create_one { query }
                }
            }
        }
    }
}

pub fn generate(model: &Model) -> TokenStream {
    let data_struct = DataStruct::new(&model);
    let set_params = SetParams::new(&model);
    let where_params = WhereParams::new(&model);
    let order_by_params = OrderByParams::new(&model);
    let pagination_params = PaginationParams::new(&model);
    let outputs = Outputs::new(&model);
    let with_params = WithParams::new(&model, &outputs);
    let query_structs = QueryStructs::new(&model, &set_params, &where_params, &with_params, &order_by_params);
    let actions = Actions::new(
        &model,
        &where_params,
        &set_params,
        &with_params,
        &outputs,
        &data_struct,
        &order_by_params,
        &pagination_params
    );

    let data_struct = data_struct.quote();
    let where_params = where_params.quote();
    let with_params = with_params.quote();
    let set_params = set_params.quote();
    let order_by_params = order_by_params.quote();
    let pagination_params = pagination_params.quote();
    let outputs_fn = outputs.quote();
    let query_structs = query_structs.quote();
    let actions = actions.quote();

    quote! {
        #outputs_fn

        #data_struct

        #query_structs

        #where_params

        #with_params

        #set_params
        
        #order_by_params
        
        #pagination_params

        #actions
    }
}
