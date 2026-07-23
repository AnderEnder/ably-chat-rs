# \MessageReactionsApi

All URIs are relative to *https://rest.ably.io*

Method | HTTP request | Description
------------- | ------------- | -------------
[**delete_message_reaction**](MessageReactionsApi.md#delete_message_reaction) | **DELETE** /chat/v4/rooms/{roomName}/messages/{serial}/reactions | Remove a message reaction
[**get_client_reactions**](MessageReactionsApi.md#get_client_reactions) | **GET** /chat/v4/rooms/{roomName}/messages/{serial}/client-reactions | Get a client's reactions on a message
[**send_message_reaction**](MessageReactionsApi.md#send_message_reaction) | **POST** /chat/v4/rooms/{roomName}/messages/{serial}/reactions | Add a message reaction



## delete_message_reaction

> delete_message_reaction(room_name, serial, r#type, x_ably_version, name)
Remove a message reaction

Removes a reaction from a message. For `unique` reactions the `name` is not required; for `distinct` and `multiple` reactions the `name` is required. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**serial** | **String** | The unique serial identifier of the message. Sent URL-encoded. | [required] |
**r#type** | [**MessageReactionType**](MessageReactionType.md) | The reaction type to remove. | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]
**name** | Option<**String**> | The reaction name (e.g. the emoji) to remove. Required for all reaction types except `unique`.  |  |

### Return type

 (empty response body)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_client_reactions

> models::MessageReactions get_client_reactions(room_name, serial, x_ably_version, for_client_id)
Get a client's reactions on a message

Returns the reaction summary filtered to a single client. Useful when a message's reaction summary is clipped (too many reacting clients) and you need to determine whether a specific client has reacted. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**serial** | **String** | The unique serial identifier of the message. Sent URL-encoded. | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]
**for_client_id** | Option<**String**> | The client ID to filter reactions by. Defaults to the client ID of the authenticated caller.  |  |

### Return type

[**models::MessageReactions**](MessageReactions.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## send_message_reaction

> send_message_reaction(room_name, serial, send_message_reaction_request, x_ably_version)
Add a message reaction

Adds a reaction to a message. Behaviour depends on the reaction `type`: `unique` (at most one reaction per client), `distinct` (at most one of each named reaction per client), `multiple` (repeatable, counted by `count`). 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**serial** | **String** | The unique serial identifier of the message. Sent URL-encoded. | [required] |
**send_message_reaction_request** | [**SendMessageReactionRequest**](SendMessageReactionRequest.md) |  | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]

### Return type

 (empty response body)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

